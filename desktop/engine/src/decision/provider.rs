//! Decision model providers. Every one speaks the System One wire:
//! `POST {base}/v1/systemone` with `{model, state, questions}`; a choice answer
//! carries `probabilities` per option and a `confidence`.

use super::options::{clip, TypedOption};
use super::{detect, DecisionSettings, Unavailable, Verdict};
use crate::http::{self, ErrorClass};
use crate::keys::Keys;
use serde_json::{json, Map, Value};
use std::time::{Duration, Instant};

pub const JEV_URL: &str = "https://api.typesafe.ai";
pub const JEV_MODEL: &str = "jev-latest";
/// The question id. Ids are for code; the model never sees it.
pub const QUESTION: &str = "best_option";

/// What a decision model returns for the one choice question.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// (option key, probability), any order.
    pub probabilities: Vec<(String, f64)>,
    pub confidence: Option<f64>,
    /// The model the server says answered.
    pub model: String,
    pub latency_ms: Option<u64>,
}

/// One decision model. Tests implement this with a fake.
pub trait DecisionModel: Send + Sync {
    /// Display name: "Jev", "Jebadiah", or the slot's name.
    fn name(&self) -> &str;
    /// Where it is asked. Never a key.
    fn endpoint(&self) -> &str;
    fn choose(&self, state: &Value, instructions: &str, options: &[TypedOption]) -> Result<Answer, String>;
}

/// The question every provider gets. Backticked paths point into `state`.
pub fn instructions() -> String {
    "A council of AI models debated `question`. Their final positions are in `positions` and the chair's verdict, if any, is in `chair_verdict`. \
Judge on the merits of the arguments, not on how many councilors hold a view. Which option is the best answer to `question`? \
Choose more_information only when the arguments do not yet support choosing any option."
        .into()
}

/// The state the decision model reads. Long positions are clipped.
pub fn state_for(v: &Verdict, _options: &[TypedOption]) -> Value {
    let positions: Vec<Value> = v.positions.iter().map(|(l, b)| json!({"seat": l, "position": clip(b, 2500)})).collect();
    let mut s = json!({"question": v.question, "positions": positions});
    if let Some(c) = v.context {
        s["context"] = json!(clip(c, 4000));
    }
    if let Some(c) = v.chair {
        s["chair_verdict"] = json!(clip(c, 2500));
    }
    if let Some(c) = v.consensus {
        s["chair_declared_consensus"] = json!(c);
    }
    s
}

/// The request body for one choice question.
pub fn request_body(model: &str, state: &Value, instructions: &str, options: &[TypedOption]) -> Value {
    let mut criteria = Map::new();
    for o in options {
        criteria.insert(o.key.clone(), json!(o.text));
    }
    json!({"model": model, "state": state, "questions": {QUESTION: {"type": "choice", "instructions": instructions, "criteria": criteria}}})
}

/// Read the choice answer. Err when it is missing or malformed.
pub fn parse_answer(v: &Value, fallback_model: &str) -> Result<Answer, String> {
    let a = v.get("answers").and_then(|a| a.get(QUESTION)).ok_or("The decision model sent no answer.")?;
    let probs = a.get("probabilities").and_then(|p| p.as_object()).ok_or("The decision model sent no probabilities.")?;
    let probabilities: Vec<(String, f64)> = probs.iter().filter_map(|(k, p)| p.as_f64().map(|p| (k.clone(), p))).collect();
    if probabilities.is_empty() {
        return Err("The decision model sent no probabilities.".into());
    }
    Ok(Answer {
        probabilities,
        confidence: a.get("confidence").and_then(|c| c.as_f64()),
        model: v.get("model").and_then(|m| m.as_str()).unwrap_or(fallback_model).to_string(),
        latency_ms: v.get("latency_ms").and_then(|l| l.as_f64()).map(|l| l.round() as u64),
    })
}

/// A System One server: Jev, Jebadiah or any other.
pub struct SystemOne {
    pub name: String,
    /// Base URL without the route, for example `https://api.typesafe.ai`.
    pub base: String,
    pub model: String,
    pub key: Option<String>,
    pub timeout: Duration,
    endpoint: String,
}

impl SystemOne {
    pub fn new(name: &str, base: &str, model: &str, key: Option<String>, timeout: Duration) -> SystemOne {
        let base = base.trim().trim_end_matches('/').trim_end_matches("/v1/systemone").trim_end_matches("/v1").to_string();
        SystemOne { name: name.into(), endpoint: format!("{base}/v1/systemone"), base, model: model.into(), key, timeout }
    }
}

impl DecisionModel for SystemOne {
    fn name(&self) -> &str {
        &self.name
    }
    fn endpoint(&self) -> &str {
        &self.endpoint
    }
    fn choose(&self, state: &Value, instructions: &str, options: &[TypedOption]) -> Result<Answer, String> {
        let body = request_body(&self.model, state, instructions, options);
        let headers = self.key.as_deref().map(http::bearer).unwrap_or_default();
        let t0 = Instant::now();
        match http::send("POST", &self.endpoint, &headers, Some(&body), self.timeout) {
            Ok(v) => {
                let mut a = parse_answer(&v, &self.model)?;
                a.latency_ms.get_or_insert(t0.elapsed().as_millis() as u64);
                Ok(a)
            }
            Err(e) => Err(match e.class {
                ErrorClass::BadKey if self.key.is_none() => "It needs a key. Add one in Settings.".to_string(),
                ErrorClass::BadKey => "That key did not work. Check you copied all of it.".to_string(),
                ErrorClass::Network => format!("Could not reach {}.", self.endpoint),
                ErrorClass::Other if e.status == Some(422) || e.status == Some(400) => format!("It refused the request ({}).", clip(&e.detail, 160)),
                c => c.message().replace("That AI", "The decision model"),
            }),
        }
    }
}

/// Environment names and the keychain account for each provider's key.
pub fn key_names(id: &str) -> (&'static [&'static str], &'static str) {
    match id {
        "jev" => (&["TYPESAFE_API_KEY", "JEV_API_KEY"], "typesafe"),
        "jebadiah" => (&["JEBADIAH_API_KEY", "AINODE_API_KEY"], "jebadiah"),
        _ => (&["SYSTEMONE_API_KEY"], "systemone"),
    }
}

/// The key for a provider: environment, then keychain, then the CLI key file.
pub fn key_for(id: &str, keys: &Keys) -> Option<String> {
    let (envs, account) = key_names(id);
    let nonempty = |v: &&String| !v.trim().is_empty();
    envs.iter()
        .find_map(|k| keys.env.get(*k).filter(nonempty).cloned())
        .or_else(|| keys.store.get(account))
        .or_else(|| envs.iter().find_map(|k| keys.file.get(*k).filter(nonempty).cloned()))
}

fn setting(v: &Option<String>) -> Option<String> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn display_name(id: &str, s: &DecisionSettings) -> String {
    match id {
        "jev" => "Jev".into(),
        "jebadiah" => "Jebadiah".into(),
        _ => setting(&s.name).unwrap_or_else(|| "System One model".into()),
    }
}

/// Seconds for one decision call. `ROUNDTABLE_DECISION_TIMEOUT` overrides.
pub fn call_timeout(keys: &Keys) -> Duration {
    Duration::from_secs(keys.env.get("ROUNDTABLE_DECISION_TIMEOUT").and_then(|v| v.parse().ok()).unwrap_or(60))
}

/// Build the chosen provider from settings, the environment and what is on this computer.
pub fn resolve(id: &str, s: &DecisionSettings, keys: &Keys) -> Result<Box<dyn DecisionModel>, Unavailable> {
    let name = display_name(id, s);
    let fail = |m: &str| Unavailable { provider: name.clone(), message: m.into() };
    let key = key_for(id, keys);
    let timeout = call_timeout(keys);
    let env = |k: &str| keys.env.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    match id {
        "jev" => {
            let Some(key) = key else {
                return Err(fail("Add your Jev key (TypeSafe) in Settings, or set TYPESAFE_API_KEY."));
            };
            let base = env("ROUNDTABLE_JEV_URL").unwrap_or_else(|| JEV_URL.into());
            let model = setting(&s.model).unwrap_or_else(|| JEV_MODEL.into());
            Ok(Box::new(SystemOne::new(&name, &base, &model, Some(key), timeout)))
        }
        "jebadiah" => {
            let url = setting(&s.url).or_else(|| env("ROUNDTABLE_JEBADIAH_URL"));
            let found = match &url {
                Some(u) => detect::probe(u, key.as_deref(), "your address").ok(),
                None => detect::find_jebadiah(keys, key.as_deref()).found,
            };
            let base = match (&url, &found) {
                (_, Some(f)) => f.url.clone(),
                (Some(u), None) => u.clone(),
                (None, None) => return Err(fail("Jebadiah is not running on this computer. Start it with `jeb serve` (pip install jebadiah-decide), or enter its address in Settings.")),
            };
            let model = setting(&s.model).or_else(|| found.map(|f| f.model)).unwrap_or_else(|| "jebadiah".into());
            Ok(Box::new(SystemOne::new(&name, &base, &model, key, timeout)))
        }
        "systemone" => {
            let Some(url) = setting(&s.url).or_else(|| env("ROUNDTABLE_SYSTEMONE_URL")) else {
                return Err(fail("Enter the model's address and model id in Settings."));
            };
            let Some(model) = setting(&s.model).or_else(|| env("ROUNDTABLE_SYSTEMONE_MODEL")) else {
                return Err(fail("Enter the model id in Settings."));
            };
            Ok(Box::new(SystemOne::new(&name, &url, &model, key, timeout)))
        }
        other => Err(Unavailable { provider: other.into(), message: "Unknown decision model.".into() }),
    }
}
