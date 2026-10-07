//! Account identity only. Credentials are read in memory and never persisted or emitted.
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const LEGACY_ACCOUNT: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub account_key: String,
    pub label: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub is_legacy: bool,
}

impl AccountProfile {
    pub fn legacy() -> Self {
        Self {
            account_key: LEGACY_ACCOUNT.into(),
            label: "Earlier history (unassigned)".into(),
            email: None,
            plan_type: None,
            is_legacy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountContext {
    pub active_account: Option<AccountProfile>,
    pub accounts: Vec<AccountProfile>,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

pub fn identity(account_id: &str, user_id: &str) -> AccountProfile {
    // Include the user and quota account/workspace. Neither an email change nor
    // a plan upgrade creates another account; different workspaces remain distinct.
    let bytes = serde_json::to_vec(&(account_id, user_id)).unwrap();
    let key = format!("chatgpt:{:x}", Sha256::digest(bytes));
    AccountProfile {
        label: format!("Account {}", &key[key.len() - 8..]),
        account_key: key,
        email: None,
        plan_type: None,
        is_legacy: false,
    }
}

pub fn from_session_metadata(value: &Value) -> Option<AccountProfile> {
    Some(identity(
        text(value, "creator_account_id")?,
        text(value, "creator_user_id")?,
    ))
}

pub(crate) fn from_auth(value: &Value) -> Option<AccountProfile> {
    if text(value, "auth_mode").is_some_and(|mode| mode.eq_ignore_ascii_case("apikey")) {
        return None;
    }
    let tokens = value.get("tokens")?;
    let encoded = text(tokens, "id_token")?.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.trim_end_matches('='))
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    let ids = &claims["https://api.openai.com/auth"];
    let mut profile = identity(
        text(tokens, "account_id").or_else(|| text(ids, "chatgpt_account_id"))?,
        text(ids, "chatgpt_user_id").or_else(|| text(ids, "user_id"))?,
    );
    profile.email = text(&claims, "email").map(str::to_owned);
    profile.plan_type = text(ids, "chatgpt_plan_type").map(str::to_owned);
    if let Some(email) = &profile.email {
        profile.label = email.clone();
    }
    Some(profile)
}

pub fn local_account() -> Result<Option<AccountProfile>, String> {
    let home = crate::session_history::sessions_dir()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .ok_or("Cannot locate Codex account")?;
    local_account_at(&home)
}

pub(crate) fn local_account_at(home: &std::path::Path) -> Result<Option<AccountProfile>, String> {
    let bytes = match std::fs::read(home.join("auth.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Could not read the Codex account identity".into()),
    };
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Codex account is being updated; retrying".to_string())?;
    Ok(from_auth(&value))
}

pub fn from_response(
    value: &Value,
    local: Option<AccountProfile>,
) -> Result<Option<AccountProfile>, String> {
    let account = &value["account"];
    if text(account, "type") != Some("chatgpt") {
        return Ok(None);
    }
    let email = text(account, "email");
    let mut profile = match local {
        Some(profile) => {
            if profile
                .email
                .as_deref()
                .zip(email)
                .is_some_and(|(a, b)| !a.eq_ignore_ascii_case(b))
            {
                return Err("Codex account changed while connecting; reconnecting".into());
            }
            profile
        }
        None => {
            // Keychain-backed installs may expose only the account/read email.
            // Keep that identity separate from ID-backed records, never guess a workspace.
            let Some(email) = email else {
                return Ok(None);
            };
            let key = format!(
                "chatgpt-email:{:x}",
                Sha256::digest(email.to_lowercase().as_bytes())
            );
            AccountProfile {
                account_key: key,
                label: email.into(),
                email: Some(email.into()),
                plan_type: None,
                is_legacy: false,
            }
        }
    };
    if let Some(email) = email {
        profile.email = Some(email.into());
        profile.label = email.into();
    }
    profile.plan_type = text(account, "planType").map(str::to_owned);
    Ok(Some(profile))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_separate_users_and_workspaces_and_match_rollout_ownership() {
        let a = identity("workspace-a", "user-a");
        assert_ne!(a.account_key, identity("workspace-a", "user-b").account_key);
        assert_ne!(a.account_key, identity("workspace-b", "user-a").account_key);
        assert_eq!(
            from_session_metadata(
                &serde_json::json!({"creator_account_id":"workspace-a","creator_user_id":"user-a"})
            )
            .unwrap()
            .account_key,
            a.account_key
        );
        assert!(from_session_metadata(
            &serde_json::json!({"creator_account_id":null,"creator_user_id":null})
        )
        .is_none());
    }
    #[test]
    fn tokens_are_never_exposed_and_plan_changes_keep_the_same_identity() {
        let claims = serde_json::json!({"email":"a@example.test", "https://api.openai.com/auth":{"chatgpt_user_id":"user-a","chatgpt_account_id":"workspace-a","chatgpt_plan_type":"plus"}});
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let local = from_auth(&serde_json::json!({"tokens":{"id_token":format!("header.{encoded}.signature"),"account_id":"workspace-a","access_token":"secret-access","refresh_token":"secret-refresh"}})).unwrap();
        let updated = from_response(&serde_json::json!({"account":{"type":"chatgpt","email":"a@example.test","planType":"pro"}}),Some(local.clone())).unwrap().unwrap();
        assert_eq!(updated.account_key, local.account_key);
        let json = serde_json::to_string(&updated).unwrap();
        assert!(
            !json.contains("secret") && !json.contains("id_token") && !json.contains("signature")
        );
        assert!(from_response(
            &serde_json::json!({"account":{"type":"chatgpt","email":"b@example.test"}}),
            Some(local)
        )
        .is_err());
        assert!(from_response(&serde_json::json!({"account":null}), None)
            .unwrap()
            .is_none());
    }
}
