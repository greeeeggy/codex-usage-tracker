//! Prices are read from OpenAI's published Markdown, never from a model allowlist.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};
use tauri::Emitter;

pub const SOURCE: &str = "https://developers.openai.com/api/docs/pricing.md";
const REFRESH_SECONDS: i64 = 6 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rates {
    pub input: f64,
    pub cached_input: Option<f64>,
    pub cache_write: Option<f64>,
    pub output: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub standard: Rates,
    pub long_context: Option<Rates>,
    pub long_context_threshold: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub source: String,
    pub fetched_at: Option<i64>,
    pub models: BTreeMap<String, ModelPrice>,
    pub last_error: Option<String>,
}

static CATALOG: OnceLock<RwLock<Catalog>> = OnceLock::new();
static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn catalog() -> Catalog {
    CATALOG
        .get_or_init(|| {
            RwLock::new(Catalog {
                source: SOURCE.into(),
                ..Catalog::default()
            })
        })
        .read()
        .unwrap()
        .clone()
}

pub fn initialize(db: &crate::db::Db) {
    if let Ok(Some(json)) = db.get_setting("pricing_catalog") {
        if let Ok(saved) = serde_json::from_str::<Catalog>(&json) {
            if saved.source == SOURCE && !saved.models.is_empty() {
                *CATALOG
                    .get_or_init(|| RwLock::new(Catalog::default()))
                    .write()
                    .unwrap() = saved;
            }
        }
    }
}

fn money(cell: &str) -> Option<f64> {
    let value = cell
        .trim()
        .strip_prefix('$')?
        .replace(',', "")
        .parse::<f64>()
        .ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

pub fn parse(markdown: &str) -> Result<BTreeMap<String, ModelPrice>, String> {
    let mut models = BTreeMap::new();
    let mut header: Vec<String> = Vec::new();
    let mut standard = true;
    // The context threshold is published alongside the table. Do not infer one
    // for an unfamiliar pricing schema.
    let threshold = markdown.lines().find_map(|line| {
        let tail = line.split("Long context: >").nth(1)?;
        let number: String = tail
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let value: f64 = number.parse().ok()?;
        Some(
            (value
                * if tail[number.len()..].starts_with('K') {
                    1000.0
                } else {
                    1.0
                }) as i64,
        )
    });
    for line in markdown.lines() {
        let line = line.trim();
        if !line.starts_with('|') && line.ends_with("models") {
            standard = true;
            header.clear();
        }
        match line {
            "Standard" | "### Standard pricing data" => {
                standard = true;
                header.clear();
            }
            "Batch" | "Flex" | "Fast" | "Ultrafast" => {
                standard = false;
                header.clear();
            }
            _ => {}
        }
        if !line.starts_with('|') {
            continue;
        }
        let row = cells(line);
        if row.iter().any(|c| *c == "Model") {
            header = row.iter().map(|c| c.to_ascii_lowercase()).collect();
            continue;
        }
        if !standard || row.len() != header.len() || header.iter().any(|c| c == "modality") {
            continue;
        }
        let column = |name: &str| header.iter().position(|c| c == name);
        let Some(model_column) = column("model") else {
            continue;
        };
        let model = row[model_column]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('`');
        if model.is_empty() || model.starts_with('-') {
            continue;
        }
        let rates = |prefix: &str| -> Option<Rates> {
            let read =
                |label: &str| column(&format!("{prefix}{label}")).and_then(|i| money(row[i]));
            Some(Rates {
                input: read("input")?,
                cached_input: read("cached input"),
                cache_write: read("cache writes"),
                output: read("output")?,
            })
        };
        let has_long = column("short context input").is_some();
        let Some(base) = rates(if has_long { "short context " } else { "" }) else {
            continue;
        };
        let long = if has_long {
            rates("long context ")
        } else {
            None
        };
        if long.is_some() && threshold.is_none() {
            return Err("Pricing context threshold was not recognized".into());
        }
        models
            .entry(model.to_ascii_lowercase())
            .or_insert(ModelPrice {
                standard: base,
                long_context: long,
                long_context_threshold: if has_long { threshold } else { None },
            });
    }
    if models.is_empty() {
        return Err("No standard text model prices found on the official pricing page".into());
    }
    Ok(models)
}

pub fn lookup<'a>(prices: &'a Catalog, model: &str) -> Option<&'a ModelPrice> {
    let name = model.to_ascii_lowercase();
    prices.models.get(&name).or_else(|| {
        // Only dated snapshots may inherit a documented base model's price.
        // Never substring-match: e.g. an unlisted pro model is not a base model.
        let (base, date) = name.rsplit_once('-')?;
        if date.len() != 2 {
            return None;
        }
        let (base, month) = base.rsplit_once('-')?;
        let (base, year) = base.rsplit_once('-')?;
        if month.len() != 2
            || year.len() != 4
            || ![date, month, year]
                .iter()
                .all(|s| s.chars().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        prices.models.get(base)
    })
}

pub fn estimate(
    usage: &crate::session_history::DetailedTokenUsage,
    price: &ModelPrice,
    per_request: bool,
) -> Option<f64> {
    let long = per_request
        && price
            .long_context_threshold
            .is_some_and(|t| usage.input_tokens > t);
    let rates = if long {
        price.long_context.as_ref().unwrap_or(&price.standard)
    } else {
        &price.standard
    };
    let cached = if usage.cached_input_tokens > 0 {
        rates.cached_input?
    } else {
        0.0
    };
    let write = rates.cache_write.unwrap_or(rates.input);
    Some(
        (usage.uncached_input_tokens as f64 * rates.input
            + usage.cached_input_tokens as f64 * cached
            + usage.cache_write_input_tokens as f64 * write
            + usage.output_tokens as f64 * rates.output)
            / 1_000_000.0,
    )
}

pub async fn refresh(db: &crate::db::Db, force: bool) -> Result<Catalog, String> {
    let _guard = REFRESH_LOCK.lock().await;
    let current = catalog();
    let now = chrono::Utc::now().timestamp();
    if !force
        && current
            .fetched_at
            .is_some_and(|t| now - t < REFRESH_SECONDS)
    {
        return Ok(current);
    }
    let result = async {
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?
            .get(SOURCE)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let body = response.text().await.map_err(|e| e.to_string())?;
        if body.len() > 2_000_000 {
            return Err("Pricing page exceeded the expected size".into());
        }
        let models = parse(&body)?;
        // A malformed or radically changed page must not erase a good cache.
        if !current.models.is_empty() && models.len() < current.models.len() / 2 {
            return Err("Pricing page was incomplete; keeping the saved prices".into());
        }
        let updated = Catalog {
            source: SOURCE.into(),
            fetched_at: Some(now),
            models,
            last_error: None,
        };
        db.set_setting(
            "pricing_catalog",
            &serde_json::to_string(&updated).map_err(|e| e.to_string())?,
        )?;
        Ok(updated)
    }
    .await;
    let updated = match result {
        Ok(value) => value,
        Err(error) => {
            let mut saved = current;
            saved.last_error = Some(error.clone());
            *CATALOG
                .get_or_init(|| RwLock::new(Catalog::default()))
                .write()
                .unwrap() = saved;
            return Err(error);
        }
    };
    *CATALOG
        .get_or_init(|| RwLock::new(Catalog::default()))
        .write()
        .unwrap() = updated.clone();
    crate::session_history::invalidate_price_cache();
    Ok(updated)
}

pub async fn run(db: Arc<crate::db::Db>, app: tauri::AppHandle) {
    loop {
        let _ = refresh(&db, false).await;
        let _ = app.emit("pricing-updated", catalog());
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PAGE: &str = "Standard\n| Model | Short context input | Short context cached input | Short context cache writes | Short context output | Long context input | Long context cached input | Long context cache writes | Long context output |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n| future-model | $2 | $0.10 | $2.50 | $10 | $4 | $0.20 | $5 | $15 |\nBatch\n| Model | Input | Cached input | Output |\n| future-model | $1 | $0.05 | $5 |\nLong context: >272K input tokens.\nStandard\n| Category | Model | Input | Cached input | Output |\n| Codex | new-coder | $3 | $0.3 | $12 |";
    #[test]
    fn discovers_new_models_and_ignores_batch_prices() {
        let prices = parse(PAGE).unwrap();
        assert_eq!(prices["future-model"].standard.input, 2.0);
        assert_eq!(prices["future-model"].long_context_threshold, Some(272_000));
        assert_eq!(prices["new-coder"].standard.output, 12.0);
        assert_eq!(
            parse(&PAGE.replace("$2 |", "$7 |")).unwrap()["future-model"]
                .standard
                .input,
            7.0
        );
    }
    #[test]
    fn exact_names_do_not_price_unknown_variants() {
        let prices = Catalog {
            models: parse(PAGE).unwrap(),
            ..Catalog::default()
        };
        assert!(lookup(&prices, "future-model-pro").is_none());
        assert!(lookup(&prices, "future-model-2026-10-02").is_some());
        assert!(parse("<html>Unavailable</html>").is_err());
    }
    #[test]
    fn uses_published_cache_write_and_long_context_rates() {
        let prices = parse(PAGE).unwrap();
        let usage = crate::session_history::DetailedTokenUsage {
            input_tokens: 300_000,
            uncached_input_tokens: 100_000,
            cached_input_tokens: 100_000,
            cache_write_input_tokens: 100_000,
            output_tokens: 10_000,
            ..Default::default()
        };
        assert!((estimate(&usage, &prices["future-model"], true).unwrap() - 1.07).abs() < 1e-9);
    }
}

#[cfg(test)]
mod live_source_tests {
    #[tokio::test]
    async fn official_pricing_source_is_readable_and_parses() {
        if std::env::var("CI").is_err() {
            return;
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let response = client
            .get(super::SOURCE)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let models = super::parse(&response.text().await.unwrap()).unwrap();
        assert!(models.len() > 10);
    }
}
