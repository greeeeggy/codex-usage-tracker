//! Saved credentials stay local, encrypted with Windows DPAPI; only metadata crosses IPC.
use crate::accounts::{self, AccountProfile};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};

#[repr(C)]
struct Blob {
    size: u32,
    data: *mut u8,
}
#[link(name = "crypt32")]
unsafe extern "system" {
    fn CryptProtectData(
        input: *const Blob,
        description: *const u16,
        entropy: *const Blob,
        reserved: *mut u8,
        prompt: *mut u8,
        flags: u32,
        output: *mut Blob,
    ) -> i32;
    fn CryptUnprotectData(
        input: *const Blob,
        description: *mut *mut u16,
        entropy: *const Blob,
        reserved: *mut u8,
        prompt: *mut u8,
        flags: u32,
        output: *mut Blob,
    ) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(memory: *mut u8) -> *mut u8;
    fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
}
pub(crate) fn crypt(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    let input = Blob {
        size: bytes
            .len()
            .try_into()
            .map_err(|_| "Credentials too large")?,
        data: bytes.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        size: 0,
        data: std::ptr::null_mut(),
    };
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                1,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(
                "Windows could not protect or unlock this login for the current user".into(),
            );
        }
        let result = std::slice::from_raw_parts(output.data, output.size as usize).to_vec();
        LocalFree(output.data);
        Ok(result)
    }
}
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let parent = path.parent().ok_or("Missing storage directory")?;
    fs::create_dir_all(parent).map_err(|_| "Cannot create storage directory")?;
    let temp = parent.join(format!(".meter-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|_| "Cannot write local file")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot save local file")?;
        drop(file);
        let src: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let dst: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), 1 | 8) } == 0 {
            return Err("Cannot replace local file".into());
        }
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result
}
pub(crate) fn codex_home() -> Result<PathBuf, String> {
    crate::session_history::sessions_dir()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .ok_or("Cannot locate CODEX_HOME".into())
}
pub(crate) fn profile(bytes: &[u8]) -> Result<AccountProfile, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "Login file is invalid")?;
    let profile = accounts::from_auth(&value).ok_or("A file-based ChatGPT login is required. Keyring-only and API-key accounts cannot be saved here.")?;
    let tokens = &value["tokens"];
    if !["access_token", "refresh_token", "id_token"]
        .iter()
        .all(|k| tokens[k].as_str().is_some_and(|s| !s.is_empty()))
    {
        return Err("Login is incomplete; sign in again".into());
    }
    Ok(profile)
}

fn require_file_storage(home: &Path) -> Result<(), String> {
    match fs::read_to_string(home.join("config.toml")) {
        Ok(text) => {
            let config: toml::Value = toml::from_str(&text)
                .map_err(|_| "Codex config.toml is invalid; switching was stopped")?;
            if config
                .get("cli_auth_credentials_store")
                .and_then(toml::Value::as_str)
                .is_some_and(|v| v != "file")
            {
                return Err("Switching requires file-based Codex login storage. Set cli_auth_credentials_store = \"file\" in Codex settings and sign in once before saving this account.".into());
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cannot read Codex credential-storage settings".into()),
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLogin {
    pub id: String,
    pub label: String,
    pub account: AccountProfile,
    pub saved_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchStatus {
    pub profiles: Vec<SavedLogin>,
    pub active_account: Option<AccountProfile>,
    pub login_pending: bool,
    pub live: crate::live_switch::LiveStatus,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    pub account: AccountProfile,
    pub message: String,
}
struct Login {
    child: Child,
    home: PathBuf,
    label: String,
}
#[derive(Default)]
pub struct SwitchManager {
    pub(crate) operation: tokio::sync::Mutex<()>,
    login: Mutex<Option<Login>>,
}
fn root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("saved-logins"))
        .map_err(|_| "Cannot locate login storage".into())
}
pub(crate) fn list(directory: &Path) -> Result<Vec<SavedLogin>, String> {
    match fs::read(directory.join("profiles.json")) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|_| "Saved account metadata is damaged".into())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err("Cannot read saved accounts".into()),
    }
}
pub(crate) fn save(directory: &Path, bytes: &[u8], label: &str) -> Result<SavedLogin, String> {
    let account = profile(bytes)?;
    let mut profiles = list(directory)?;
    let previous = profiles
        .iter()
        .find(|p| p.account.account_key == account.account_key);
    let label = label.trim();
    if label.len() > 100 {
        return Err("Account name must be 100 characters or less".into());
    }
    let saved = SavedLogin {
        id: previous
            .map(|p| p.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        label: if label.is_empty() {
            previous
                .map(|p| p.label.clone())
                .unwrap_or_else(|| account.label.clone())
        } else {
            label.into()
        },
        account,
        saved_at: chrono::Utc::now().to_rfc3339(),
    };
    atomic_write(
        &directory.join(format!("{}.dpapi", saved.id)),
        &crypt(bytes, true)?,
    )?;
    profiles.retain(|p| p.id != saved.id);
    profiles.push(saved.clone());
    atomic_write(
        &directory.join("profiles.json"),
        &serde_json::to_vec_pretty(&profiles).unwrap(),
    )?;
    Ok(saved)
}
pub(crate) fn validate_id(id: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| "Invalid account selection".into())
}
#[tauri::command]
pub async fn get_switch_accounts(
    app: tauri::AppHandle,
    manager: tauri::State<'_, SwitchManager>,
) -> Result<SwitchStatus, String> {
    let login_pending = manager.login.lock().unwrap().is_some();
    let live = crate::live_switch::status().await?;
    Ok(SwitchStatus {
        profiles: list(&root(&app)?)?,
        active_account: accounts::local_account()?,
        login_pending,
        live,
    })
}
#[tauri::command]
pub async fn save_current_login(
    app: tauri::AppHandle,
    manager: tauri::State<'_, SwitchManager>,
    label: String,
) -> Result<SavedLogin, String> {
    let _guard = manager
        .operation
        .try_lock()
        .map_err(|_| "An account operation is already in progress")?;
    require_file_storage(&codex_home()?)?;
    let bytes = fs::read(codex_home()?.join("auth.json"))
        .map_err(|_| "No file-based login found. Sign into Codex first.")?;
    save(&root(&app)?, &bytes, &label)
}
#[tauri::command]
pub async fn remove_saved_login(
    app: tauri::AppHandle,
    manager: tauri::State<'_, SwitchManager>,
    id: String,
) -> Result<(), String> {
    let _guard = manager
        .operation
        .try_lock()
        .map_err(|_| "An account operation is already in progress")?;
    validate_id(&id)?;
    let directory = root(&app)?;
    let mut profiles = list(&directory)?;
    profiles.retain(|p| p.id != id);
    atomic_write(
        &directory.join("profiles.json"),
        &serde_json::to_vec_pretty(&profiles).unwrap(),
    )?;
    match fs::remove_file(directory.join(format!("{id}.dpapi"))) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(
            "Account removed from the list, but Windows could not delete its encrypted login"
                .into(),
        ),
    }
}
#[tauri::command]
pub async fn start_account_login(
    app: tauri::AppHandle,
    manager: tauri::State<'_, SwitchManager>,
    label: String,
) -> Result<(), String> {
    let _guard = manager
        .operation
        .try_lock()
        .map_err(|_| "An account operation is already in progress")?;
    let mut pending = manager.login.lock().unwrap();
    if pending.is_some() {
        return Err("Finish or cancel the current sign-in first".into());
    }
    if label.trim().is_empty() || label.len() > 100 {
        return Err("Enter an account name up to 100 characters".into());
    }
    let home = root(&app)?.join(format!("login-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&home).map_err(|_| "Cannot prepare sign-in")?;
    let child = Command::new(crate::codex_client::find_codex_executable()?)
        .args(["-c", "cli_auth_credentials_store=\"file\"", "login"])
        .env("CODEX_HOME", &home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|_| "Could not start Codex sign-in")?;
    *pending = Some(Login { child, home, label });
    Ok(())
}
#[tauri::command]
pub async fn poll_account_login(
    app: tauri::AppHandle,
    manager: tauri::State<'_, SwitchManager>,
) -> Result<Option<SavedLogin>, String> {
    let _guard = manager
        .operation
        .try_lock()
        .map_err(|_| "An account operation is already in progress")?;
    let mut pending = manager.login.lock().unwrap();
    let Some(login) = pending.as_mut() else {
        return Ok(None);
    };
    let Some(exit) = login.child.try_wait().map_err(|_| "Cannot check sign-in")? else {
        return Ok(None);
    };
    let login = pending.take().unwrap();
    let result = if exit.success() {
        fs::read(login.home.join("auth.json"))
            .map_err(|_| "Sign-in did not save a login".to_string())
            .and_then(|bytes| save(&root(&app)?, &bytes, &login.label))
            .map(Some)
    } else {
        Err("Sign-in was cancelled or failed. Retry Add account.".into())
    };
    let _ = fs::remove_dir_all(&login.home);
    result
}
#[tauri::command]
pub async fn cancel_account_login(manager: tauri::State<'_, SwitchManager>) -> Result<(), String> {
    let _guard = manager
        .operation
        .try_lock()
        .map_err(|_| "An account operation is already in progress")?;
    if let Some(mut login) = manager.login.lock().unwrap().take() {
        let _ = login.child.kill();
        let _ = login.child.wait();
        let _ = fs::remove_dir_all(login.home);
    }
    Ok(())
}
#[tauri::command]
pub async fn switch_codex_account(
    app: tauri::AppHandle,
    manager: tauri::State<'_, SwitchManager>,
    monitor: tauri::State<'_, crate::usage_service::MonitorController>,
    state: tauri::State<'_, Arc<tokio::sync::RwLock<crate::usage_service::UsageState>>>,
    id: String,
) -> Result<SwitchOutcome, String> {
    let _guard = manager
        .operation
        .try_lock()
        .map_err(|_| "An account operation is already in progress")?;
    if manager.login.lock().unwrap().is_some() {
        return Err("Finish or cancel sign-in first".into());
    }
    require_file_storage(&codex_home()?)?;
    validate_id(&id)?;
    let target = list(&root(&app)?)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("Saved account no longer exists")?;
    let live = crate::live_switch::status().await?;
    if live.connected == 0 {
        return Err("Enable live switching, then quit and reopen Codex normally once. Wait for Desktop connected in Switch.".into());
    }
    if live.busy {
        return Err(
            "Finish or stop active Codex work or voice before switching. Codex was left open."
                .into(),
        );
    }
    let _ = app.emit(
        "switch-progress",
        "Asking the running Codex engine to change accounts…",
    );
    monitor.pause().await?;
    let result = crate::live_switch::apply(&id).await;
    {
        let mut usage = state.write().await;
        usage.snapshot = None;
        usage.account_usage = None;
        usage.active_account = None;
    }
    let _ = app.emit(
        "accounts-updated",
        accounts::AccountContext {
            active_account: None,
            accounts: app
                .state::<Arc<crate::db::Db>>()
                .list_accounts()
                .unwrap_or_default(),
        },
    );
    monitor.resume();
    let _ = app.emit("refresh-requested", ());
    result?;
    Ok(SwitchOutcome {
        account: target.account,
        message: format!(
            "Codex is now using {}. Confirmed by its running engine; Codex stayed open.",
            target.label
        ),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dpapi_roundtrip_and_tamper_rejection() {
        let encrypted = crypt(b"test credentials", true).unwrap();
        assert_ne!(encrypted, b"test credentials");
        assert_eq!(crypt(&encrypted, false).unwrap(), b"test credentials");
        let mut damaged = encrypted;
        damaged[20] ^= 1;
        assert!(crypt(&damaged, false).is_err());
    }
    #[test]
    fn selection_cannot_traverse_storage() {
        assert!(validate_id("../auth").is_err());
        assert!(validate_id(&uuid::Uuid::new_v4().to_string()).is_ok());
    }
    #[test]
    fn atomic_replace_preserves_complete_file() {
        let dir = std::env::temp_dir().join(format!("meter-test-{}", uuid::Uuid::new_v4()));
        let path = dir.join("auth.json");
        atomic_write(&path, b"old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"new");
        fs::remove_dir_all(dir).unwrap();
    }
}
