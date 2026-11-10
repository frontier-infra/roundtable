//! Find a local Jebadiah the way AI apps are found: look where it runs.
//!
//! A System One server lists its models at `GET /v1/models`; one that lists one of
//! our builds is a Jebadiah server. Places looked, in order:
//! `ROUNDTABLE_JEBADIAH_URL`, `jeb serve` (127.0.0.1:8100), AINode (127.0.0.1:3000, :8000).
//! Ollama and LM Studio carry the weights but not the decision route; when they
//! hold Jebadiah, the hint says to run `jeb serve` in front of them.
//! `ROUNDTABLE_JEBADIAH_PROBE` (comma list of base URLs) replaces the list, for tests.
//! `ROUNDTABLE_JEBADIAH_WEIGHTS` ("Ollama=http://..,LM Studio=http://..") replaces the
//! Ollama and LM Studio list addresses, for tests.
//!
//! Only our builds count. A stranger's Hugging Face account is also named Jebadiah and
//! holds unrelated models, so a name alone proves nothing. Ours are:
//! - Hugging Face and AINode: `frontier-infra/jebadiah-*` (also `hf.co/frontier-infra/jebadiah-*`).
//! - Ollama library: `frontierinfra/jebadiah` and `jbrashearfrontierinfra/jebadiah`, any tag.
//!
//! Any other id that holds "jebadiah" is skipped, and `note` says so.

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
    /// One line when a model named Jebadiah that is not ours was skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

const PROBE_TIMEOUT: Duration = Duration::from_millis(1200);

/// Namespaces that publish our Jebadiah: Hugging Face and AINode, then the Ollama library.
/// The one list is `jebadiah-ids.json`; tests here and in the UI check against it.
pub const OURS: &[&str] = &["frontier-infra", "frontierinfra", "jbrashearfrontierinfra"];
/// Hosts dropped before the namespace is read.
pub const HOSTS: &[&str] = &["hf.co/", "huggingface.co/", "registry.ollama.ai/"];

/// True for our builds only. Hosts (`hf.co/`, `huggingface.co/`, `registry.ollama.ai/`)
/// and an Ollama tag (`:9b`) are ignored; the namespace must be ours.
pub fn is_ours(id: &str) -> bool {
    let low = id.trim().to_ascii_lowercase();
    let low = low.split(':').next().unwrap_or("");
    let low = HOSTS.iter().find_map(|h| low.strip_prefix(h)).unwrap_or(low);
    let Some((ns, name)) = low.split_once('/') else { return false };
    OURS.contains(&ns) && (name == "jebadiah" || name.starts_with("jebadiah-"))
}

/// Named like Jebadiah but not one of ours.
fn is_stranger(id: &str) -> bool {
    id.to_ascii_lowercase().contains("jebadiah") && !is_ours(id)
}

/// Preferred Jebadiah ids, ours only: the 9B first, then the 27B, then the rest.
pub fn pick(ids: &[String]) -> Option<String> {
    let jeb: Vec<&String> = ids.iter().filter(|i| is_ours(i)).collect();
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

/// Ask one base URL for its models. Ok when it lists one of our Jebadiah builds.
pub fn probe(base: &str, key: Option<&str>, via: &str) -> Result<Found, String> {
    probe_noting(base, key, via, &mut Vec::new())
}

/// `probe`, adding any unrelated Jebadiah-named ids it saw to `skipped`.
fn probe_noting(base: &str, key: Option<&str>, via: &str, skipped: &mut Vec<String>) -> Result<Found, String> {
    let base = base.trim().trim_end_matches('/').trim_end_matches("/v1/systemone").trim_end_matches("/v1").to_string();
    let headers = key.map(http::bearer).unwrap_or_default();
    match http::send("GET", &format!("{base}/v1/models"), &headers, None, PROBE_TIMEOUT) {
        Ok(v) => {
            let ids = model_ids(&v);
            skipped.extend(ids.iter().filter(|i| is_stranger(i)).cloned());
            match pick(&ids) {
                Some(model) => Ok(Found { url: base, model, via: via.into() }),
                None => Err(format!("{base}: answered, no Jebadiah model")),
            }
        }
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

/// Ollama and LM Studio list addresses.
fn weight_places(keys: &Keys) -> Vec<(String, String)> {
    if let Some(list) = keys.env.get("ROUNDTABLE_JEBADIAH_WEIGHTS") {
        return list.split(',').filter_map(|p| p.split_once('=')).map(|(a, u)| (a.trim().to_string(), u.trim().to_string())).collect();
    }
    if keys.env.contains_key("ROUNDTABLE_JEBADIAH_PROBE") {
        return Vec::new();
    }
    vec![("Ollama".into(), "http://127.0.0.1:11434/api/tags".into()), ("LM Studio".into(), "http://127.0.0.1:1234/v1/models".into())]
}

/// The one-line note for unrelated models, or None.
fn skipped_note(mut skipped: Vec<String>) -> Option<String> {
    skipped.sort();
    skipped.dedup();
    match skipped.len() {
        0 => None,
        1 => Some(format!("Skipped {}: an unrelated model named Jebadiah, not a frontier-infra build.", skipped[0])),
        n => Some(format!("Skipped {n} unrelated models named Jebadiah, not frontier-infra builds: {}.", skipped.join(", "))),
    }
}

/// Look on this computer. Takes about a second per empty place at most.
pub fn find_jebadiah(keys: &Keys, key: Option<&str>) -> Detection {
    let mut d = Detection::default();
    let mut skipped = Vec::new();
    for (url, via) in places(keys) {
        match probe_noting(&url, key, &via, &mut skipped) {
            Ok(f) => {
                d.looked.push(format!("{}: {} ({via})", f.url, f.model));
                d.found = Some(f);
                d.note = skipped_note(skipped);
                return d;
            }
            Err(e) => d.looked.push(e),
        }
    }
    // Weights without the decision route.
    for (name, url) in weight_places(keys) {
        let Ok(v) = http::send("GET", &url, &[], None, PROBE_TIMEOUT) else { continue };
        let ids = model_ids(&v);
        skipped.extend(ids.iter().filter(|i| is_stranger(i)).cloned());
        d.looked.push(format!("{name}: {} models", ids.len()));
        if pick(&ids).is_some() && d.hint.is_none() {
            d.hint = Some(format!("{name} has Jebadiah. Run `jeb serve` (pip install jebadiah-decide) so Roundtable can ask it."));
        }
    }
    d.note = skipped_note(skipped);
    if d.hint.is_none() && !keys.env.contains_key("ROUNDTABLE_JEBADIAH_PROBE") {
        d.hint = Some("Start Jebadiah with `jeb serve` (pip install jebadiah-decide), or enter its address in Settings.".into());
    }
    d
}
