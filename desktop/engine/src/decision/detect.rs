//! Find a local Jebadiah the way AI apps are found: look where it runs.
//!
//! A System One server lists its models at `GET /v1/models`; one whose id holds
//! "jebadiah" is a Jebadiah server. Places looked, in order:
//! `ROUNDTABLE_JEBADIAH_URL`, `jeb serve` (127.0.0.1:8100), AINode (127.0.0.1:3000, :8000).
//! Ollama and LM Studio carry the weights but not the decision route; when they
//! hold Jebadiah, the hint says to run `jeb serve` in front of them.
//! `ROUNDTABLE_JEBADIAH_PROBE` (comma list of base URLs) replaces the list, for tests.

use crate::http;
use crate::keys::Keys;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    pub url: String,
    pub model: String,
    /// "jeb serve", "AINode", "your address".
    pub via: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Detection {
    pub found: Option<Found>,
    /// Every place looked, with what answered.
    pub looked: Vec<String>,
    /// A plain next step when nothing usable answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

const PROBE_TIMEOUT: Duration = Duration::from_millis(1200);

/// Preferred Jebadiah ids: the 9B first, then the 27B, then the rest.
fn pick(ids: &[String]) -> Option<String> {
    let jeb: Vec<&String> = ids.iter().filter(|i| i.to_ascii_lowercase().contains("jebadiah")).collect();
    ["9b", "27b", "4b"]
        .iter()
        .find_map(|s| jeb.iter().find(|i| i.to_ascii_lowercase().contains(s)))
        .or_else(|| jeb.first())
        .map(|s| s.to_string())
}

fn model_ids(v: &serde_json::Value) -> Vec<String> {
    let list = v.get("data").or_else(|| v.get("models")).and_then(|d| d.as_array()).cloned().unwrap_or_default();
    list.iter()
        .filter_map(|m| m.get("id").or_else(|| m.get("name")).or_else(|| m.get("model")).and_then(|i| i.as_str()).map(str::to_string))
        .collect()
}

/// Ask one base URL for its models. Ok when it lists a Jebadiah model.
pub fn probe(base: &str, key: Option<&str>, via: &str) -> Result<Found, String> {
    let base = base.trim().trim_end_matches('/').trim_end_matches("/v1/systemone").trim_end_matches("/v1").to_string();
    let headers = key.map(http::bearer).unwrap_or_default();
    match http::send("GET", &format!("{base}/v1/models"), &headers, None, PROBE_TIMEOUT) {
        Ok(v) => match pick(&model_ids(&v)) {
            Some(model) => Ok(Found { url: base, model, via: via.into() }),
            None => Err(format!("{base}: answered, no Jebadiah model")),
        },
        Err(e) => Err(format!("{base}: {}", e.status.map(|s| format!("HTTP {s}")).unwrap_or_else(|| "nothing there".into()))),
    }
}

fn places(keys: &Keys) -> Vec<(String, String)> {
    if let Some(list) = keys.env.get("ROUNDTABLE_JEBADIAH_PROBE") {
        return list.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|u| (u.to_string(), "test".to_string())).collect();
    }
    let mut v = Vec::new();
    if let Some(u) = keys.env.get("ROUNDTABLE_JEBADIAH_URL").filter(|u| !u.trim().is_empty()) {
        v.push((u.clone(), "ROUNDTABLE_JEBADIAH_URL".into()));
    }
    v.push(("http://127.0.0.1:8100".into(), "jeb serve".into()));
    v.push(("http://127.0.0.1:3000".into(), "AINode".into()));
    v.push(("http://127.0.0.1:8000".into(), "AINode".into()));
    v
}

/// Look on this computer. Takes about a second per empty place at most.
pub fn find_jebadiah(keys: &Keys, key: Option<&str>) -> Detection {
    let mut d = Detection::default();
    for (url, via) in places(keys) {
        match probe(&url, key, &via) {
            Ok(f) => {
                d.looked.push(format!("{}: {} ({via})", f.url, f.model));
                d.found = Some(f);
                return d;
            }
            Err(e) => d.looked.push(e),
        }
    }
    if keys.env.contains_key("ROUNDTABLE_JEBADIAH_PROBE") {
        return d;
    }
    // Weights without the decision route.
    let ollama = http::send("GET", "http://127.0.0.1:11434/api/tags", &[], None, PROBE_TIMEOUT).ok().map(|v| model_ids(&v));
    let lmstudio = http::send("GET", "http://127.0.0.1:1234/v1/models", &[], None, PROBE_TIMEOUT).ok().map(|v| model_ids(&v));
    for (name, ids) in [("Ollama", ollama), ("LM Studio", lmstudio)] {
        if let Some(ids) = ids {
            d.looked.push(format!("{name}: {} models", ids.len()));
            if pick(&ids).is_some() && d.hint.is_none() {
                d.hint = Some(format!("{name} has Jebadiah. Run `jeb serve` (pip install jebadiah-decide) so Roundtable can ask it."));
            }
        }
    }
    if d.hint.is_none() {
        d.hint = Some("Start Jebadiah with `jeb serve` (pip install jebadiah-decide), or enter its address in Settings.".into());
    }
    d
}
