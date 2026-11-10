//! Providers the person adds (Settings, AI accounts, "Add a provider"): OpenRouter,
//! Ollama, LM Studio, Groq, Together, or any OpenAI-compatible base URL.
//!
//! Every ticked model is its own seat. Its head id is `<provider>:<model>`, for example
//! `ollama:llama3.2:latest` or `openrouter:openai/gpt-4o-mini`; the provider part ends at
//! the first colon. A seat is asked with a plain chat completion: one user message, no
//! tools, no system prompt, nothing it can run (the seat isolation rule from rc8).
//!
//! Keys live in the OS keychain like the other keys, under the account `provider-<id>`.
//! The environment may carry one too: the preset's own name (`OPENROUTER_API_KEY`, ...)
//! or `ROUNDTABLE_PROVIDER_<ID>_KEY`.
//!
//! A model named Jebadiah counts as ours only when it is a frontier-infra build
//! (`decision::detect::is_ours`); any other one is a plain model and is never offered
//! as our decision model.

pub mod cli;

use crate::clis::Fix;
use crate::council::Outcome;
use crate::http::{self, ErrorClass, HttpError};
use crate::keys::Keys;
use crate::providers::{Head, Kind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Mutex, RwLock};
use std::time::Duration;

/// A provider the app knows how to add.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    /// OpenAI-compatible base URL, ending before `/chat/completions`. Empty for Custom.
    pub base_url: &'static str,
    pub needs_key: bool,
    /// The provider's own key variable, read from the environment first.
    pub env_key: &'static str,
    /// Where a person gets a key.
    pub key_page: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset { id: "openrouter", name: "OpenRouter", base_url: "https://openrouter.ai/api/v1", needs_key: true, env_key: "OPENROUTER_API_KEY", key_page: "https://openrouter.ai/settings/keys" },
    Preset { id: "ollama", name: "Ollama", base_url: "http://127.0.0.1:11434/v1", needs_key: false, env_key: "", key_page: "" },
    Preset { id: "lmstudio", name: "LM Studio", base_url: "http://127.0.0.1:1234/v1", needs_key: false, env_key: "", key_page: "" },
    Preset { id: "groq", name: "Groq", base_url: "https://api.groq.com/openai/v1", needs_key: true, env_key: "GROQ_API_KEY", key_page: "https://console.groq.com/keys" },
    Preset { id: "together", name: "Together", base_url: "https://api.together.xyz/v1", needs_key: true, env_key: "TOGETHER_API_KEY", key_page: "https://api.together.ai/settings/api-keys" },
    Preset { id: "custom", name: "Custom", base_url: "", needs_key: false, env_key: "", key_page: "" },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    let id = id.trim().to_ascii_lowercase().replace([' ', '-', '_'], "");
    PRESETS.iter().find(|p| p.id == id)
}

/// The most tokens a provider seat may write in one answer.
pub const MAX_TOKENS: u32 = 4000;
/// The budget for the one retry when a thinking model spent all of MAX_TOKENS
/// thinking and wrote no answer (finish_reason "length", empty content).
pub const THINKING_MAX_TOKENS: u32 = 16000;
/// How long a model list may take. Local servers answer in milliseconds.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a local probe may take before the app says nothing is there.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(1200);
/// One wait before asking a busy provider again.
pub const RATE_LIMIT_WAIT: Duration = Duration::from_secs(2);

/// One ticked model: a seat. Prices are US dollars per million tokens, when the provider publishes them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct SeatModel {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_in: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_out: Option<f64>,
}

/// One added provider, as `settings.json` keeps it. Never a key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ProviderConfig {
    /// Short id: the seat prefix and the keychain account suffix (`openrouter`, `ollama`, `my-proxy`).
    pub id: String,
    /// The preset it came from, `custom` for any other base URL.
    pub preset: String,
    /// Name people see: "OpenRouter", "My proxy".
    pub name: String,
    /// OpenAI-compatible base URL, ending before `/chat/completions`.
    pub base_url: String,
    pub needs_key: bool,
    /// Ticked models. Each one is a seat.
    pub models: Vec<SeatModel>,
}

impl ProviderConfig {
    /// A provider from a preset, with its default address.
    pub fn from_preset(p: &Preset) -> ProviderConfig {
        ProviderConfig { id: p.id.into(), preset: p.id.into(), name: p.name.into(), base_url: p.base_url.into(), needs_key: p.needs_key, models: Vec::new() }
    }
    pub fn local(&self) -> bool {
        is_local_url(&self.base_url)
    }
    /// The keychain account for this provider's key.
    pub fn account(&self) -> String {
        format!("provider-{}", self.id)
    }
    /// Environment names that may carry the key, first wins.
    pub fn env_keys(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(p) = preset(&self.preset).filter(|p| !p.env_key.is_empty()) {
            v.push(p.env_key.to_string());
        }
        v.push(format!("ROUNDTABLE_PROVIDER_{}_KEY", self.id.to_ascii_uppercase().replace(['-', '.'], "_")));
        v
    }
    pub fn key_page(&self) -> &'static str {
        preset(&self.preset).map(|p| p.key_page).unwrap_or("")
    }
    /// Seat ids for the ticked models.
    pub fn seat_ids(&self) -> Vec<String> {
        self.models.iter().map(|m| seat_id(&self.id, &m.id)).collect()
    }
}

pub fn seat_id(provider: &str, model: &str) -> String {
    format!("{provider}:{model}")
}

/// Split a seat id at the first colon: (provider, model). None when either part is empty.
pub fn split_seat(id: &str) -> Option<(&str, &str)> {
    let (p, m) = id.trim().split_once(':')?;
    let (p, m) = (p.trim(), m.trim());
    if p.is_empty() || m.is_empty() || p.contains('/') {
        return None;
    }
    Some((p, m))
}

/// A provider id from a name: lower case, letters, digits and dashes.
pub fn make_id(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "custom".into()
    } else {
        out
    }
}

/// The base URL as the engine uses it: no trailing slash, no route, `/v1` when there is no path.
pub fn normalize_base(url: &str) -> String {
    let mut u = url.trim().trim_end_matches('/').to_string();
    for tail in ["/chat/completions", "/completions", "/models", "/api/tags"] {
        if let Some(s) = u.strip_suffix(tail) {
            u = s.trim_end_matches('/').to_string();
        }
    }
    if !u.contains("://") && !u.is_empty() {
        u = format!("http://{u}");
    }
    let path_start = u.find("://").map(|i| i + 3).and_then(|i| u[i..].find('/').map(|j| i + j));
    if path_start.is_none() && !u.is_empty() {
        u.push_str("/v1");
    }
    u
}

/// The host of a URL, lower case, without port or brackets.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or("");
    let hostport = hostport.rsplit_once('@').map(|(_, h)| h).unwrap_or(hostport);
    let host = if let Some(h) = hostport.strip_prefix('[') { h.split(']').next().unwrap_or("") } else { hostport.split(':').next().unwrap_or("") };
    host.to_ascii_lowercase()
}

/// True for an address on this computer: localhost, 127.x, ::1, 0.0.0.0.
pub fn is_local_url(url: &str) -> bool {
    let h = host_of(url);
    h == "localhost" || h.ends_with(".localhost") || h.starts_with("127.") || h == "::1" || h == "0.0.0.0"
}

// ---- Model lists ----

/// One model a provider lists.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct ModelInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
    /// US dollars per million input tokens, when published.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_in: Option<f64>,
    /// US dollars per million output tokens, when published.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_out: Option<f64>,
    /// Runs on this computer.
    pub free: bool,
    /// One of our frontier-infra Jebadiah builds.
    pub ours: bool,
    /// One plain line, for a model worth a word (a Jebadiah that is not ours).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok())))
}

/// Read a model list: OpenAI `{data: [...]}`, Ollama `{models: [...]}`, or a bare array (Together).
/// OpenRouter prices are per token and become per million; Together's are per million already.
pub fn parse_models(v: &Value) -> Vec<ModelInfo> {
    let list = v.as_array().or_else(|| v.get("data").and_then(|d| d.as_array())).or_else(|| v.get("models").and_then(|d| d.as_array()));
    let Some(list) = list else { return Vec::new() };
    let mut out = Vec::new();
    for m in list {
        let id = m.get("id").or_else(|| m.get("name")).or_else(|| m.get("model")).and_then(|i| i.as_str()).map(str::trim).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let mut info = ModelInfo { id: id.to_string(), ..Default::default() };
        info.name = m.get("name").or_else(|| m.get("display_name")).and_then(|n| n.as_str()).filter(|n| *n != id).map(str::to_string);
        info.context = m.get("context_length").or_else(|| m.get("context_window")).and_then(|c| c.as_u64());
        if let Some(p) = m.get("pricing") {
            // OpenRouter: prompt and completion, dollars per token, as strings.
            if let (Some(i), Some(o)) = (num(p.get("prompt")), num(p.get("completion"))) {
                info.price_in = Some(i * 1_000_000.0);
                info.price_out = Some(o * 1_000_000.0);
            } else if let (Some(i), Some(o)) = (num(p.get("input")), num(p.get("output"))) {
                // Together: dollars per million tokens.
                info.price_in = Some(i);
                info.price_out = Some(o);
            }
            // OpenRouter marks a router model with -1: no published price.
            if info.price_in.is_some_and(|p| p < 0.0) || info.price_out.is_some_and(|p| p < 0.0) {
                info.price_in = None;
                info.price_out = None;
            }
        }
        mark_jebadiah(&mut info);
        if !out.iter().any(|o: &ModelInfo| o.id == info.id) {
            out.push(info);
        }
    }
    out
}

/// The detect rule from fix/detect-ours: only frontier-infra builds are ours.
fn mark_jebadiah(m: &mut ModelInfo) {
    if crate::decision::detect::is_ours(&m.id) {
        m.ours = true;
        m.note = Some("Our Jebadiah build (frontier-infra).".into());
    } else if m.id.to_ascii_lowercase().contains("jebadiah") {
        m.ours = false;
        m.note = Some("Not a frontier-infra build. It sits as a plain seat only.".into());
    }
}

/// Models whose id or name holds every word of the search, in any case.
pub fn search<'a>(models: &'a [ModelInfo], q: &str) -> Vec<&'a ModelInfo> {
    let words: Vec<String> = q.split_whitespace().map(|w| w.to_ascii_lowercase()).collect();
    models
        .iter()
        .filter(|m| {
            let hay = format!("{} {}", m.id, m.name.as_deref().unwrap_or("")).to_ascii_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .collect()
}

fn auth_headers(cfg: &ProviderConfig, key: Option<&str>) -> Vec<(&'static str, String)> {
    let mut h = key.filter(|k| !k.trim().is_empty()).map(http::bearer).unwrap_or_default();
    if cfg.preset == "openrouter" {
        // OpenRouter's optional attribution headers.
        h.push(("HTTP-Referer", "https://roundtable.sh".into()));
        h.push(("X-Title", "Roundtable".into()));
    }
    h
}

/// The address Ollama's own list lives at, from its OpenAI-compatible base.
fn ollama_tags_url(base: &str) -> String {
    let root = base.trim_end_matches('/').trim_end_matches("/v1");
    format!("{root}/api/tags")
}

fn looks_like_ollama(cfg: &ProviderConfig) -> bool {
    cfg.preset == "ollama" || cfg.base_url.contains(":11434")
}

/// The provider's models: `GET {base}/models`, and for Ollama also `/api/tags`.
/// Sorted by id. Local models are marked free.
pub fn list_models(cfg: &ProviderConfig, key: Option<&str>, timeout: Duration) -> Result<Vec<ModelInfo>, HttpError> {
    let base = normalize_base(&cfg.base_url);
    let headers = auth_headers(cfg, key);
    let first = http::send("GET", &format!("{base}/models"), &headers, None, timeout);
    let mut models = match &first {
        Ok(v) => parse_models(v),
        Err(_) => Vec::new(),
    };
    if looks_like_ollama(cfg) {
        if let Ok(v) = http::send("GET", &ollama_tags_url(&base), &headers, None, timeout) {
            for m in parse_models(&v) {
                if !models.iter().any(|o| o.id == m.id) {
                    models.push(m);
                }
            }
        }
    }
    if models.is_empty() {
        first?;
    }
    let local = cfg.local();
    for m in &mut models {
        m.free = local;
        if local {
            m.price_in = Some(0.0);
            m.price_out = Some(0.0);
        }
    }
    models.sort_by_key(|m| m.id.to_ascii_lowercase());
    Ok(models)
}

/// A plain line for a failed model list.
pub fn list_error(cfg: &ProviderConfig, e: &HttpError) -> String {
    match e.class {
        ErrorClass::Network | ErrorClass::Timeout if cfg.local() => format!("{} is not answering at {}. Is it running?", cfg.name, normalize_base(&cfg.base_url)),
        ErrorClass::Network => format!("Could not reach {}. Check the address and your internet connection.", cfg.name),
        ErrorClass::BadKey if cfg.needs_key => format!("{} needs a working key. Check you copied all of it.", cfg.name),
        ErrorClass::BadKey => format!("{} asked for a key. Add one.", cfg.name),
        ErrorClass::Other if e.status == Some(404) => format!("{} has no model list at {}/models. Check the address.", cfg.name, normalize_base(&cfg.base_url)),
        c => c.message().replace("That AI", &cfg.name),
    }
}

// ---- Asking a seat ----

/// The chat body for a provider seat: one user message, no tools, no system prompt.
pub fn seat_body(model: &str, prompt: &str) -> Value {
    http::chat_body(model, prompt, MAX_TOKENS, None)
}

/// Ask one model once more after a short wait when the provider says it is busy.
pub fn chat(cfg: &ProviderConfig, model: &str, key: Option<&str>, prompt: &str, timeout: Duration) -> Outcome {
    let url = format!("{}/chat/completions", normalize_base(&cfg.base_url));
    let headers = auth_headers(cfg, key);
    let body = seat_body(model, prompt);
    let mut res = http::send("POST", &url, &headers, Some(&body), timeout);
    if matches!(&res, Err(e) if e.class == ErrorClass::RateLimited) {
        std::thread::sleep(rate_limit_wait());
        res = http::send("POST", &url, &headers, Some(&body), timeout);
    }
    // A thinking model can spend the whole budget thinking and send no answer
    // (finish_reason "length", empty content). Ask once more with room to answer.
    if matches!(&res, Ok(v) if out_of_room(v)) {
        let body = http::chat_body(model, prompt, THINKING_MAX_TOKENS, None);
        if let Ok(v) = http::send("POST", &url, &headers, Some(&body), timeout) {
            res = Ok(v);
        }
    }
    match res {
        Ok(v) => {
            let t = http::strip_think(&http::chat_text(&v));
            if !t.is_empty() {
                Outcome::Answered(t)
            } else if finish_reason(&v) == "length" {
                Outcome::Empty(format!("{}: used its whole answer budget thinking and wrote no answer", seat_id(&cfg.id, model)))
            } else {
                Outcome::Empty(format!("{}: empty answer", seat_id(&cfg.id, model)))
            }
        }
        Err(e) if e.class == ErrorClass::Timeout => Outcome::Timeout,
        Err(e) => match e.class {
            ErrorClass::Network if cfg.local() => Outcome::ToolFailed(Fix::Retry, format!("{} is not answering on this computer. Is it running?", cfg.name)),
            ErrorClass::Network => Outcome::ToolFailed(Fix::Retry, format!("Could not reach {}.", cfg.name)),
            ErrorClass::BadKey if key.is_none() => Outcome::Missing(format!("{} needs a key. Add it in Settings, AI accounts.", cfg.name), Fix::AddKey),
            ErrorClass::Other if e.status == Some(404) => Outcome::ToolFailed(Fix::Retry, format!("{} does not have {model} any more.", cfg.name)),
            c => Outcome::Failed(c, e.detail),
        },
    }
}

fn finish_reason(v: &Value) -> &str {
    v.pointer("/choices/0/finish_reason").and_then(|f| f.as_str()).unwrap_or("")
}

/// True when the answer is empty because the model ran out of tokens.
fn out_of_room(v: &Value) -> bool {
    finish_reason(v) == "length" && http::strip_think(&http::chat_text(v)).is_empty()
}

fn rate_limit_wait() -> Duration {
    std::env::var("ROUNDTABLE_RATE_LIMIT_WAIT_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(RATE_LIMIT_WAIT)
}

/// The key for a provider: the environment, then the keychain.
pub fn key_for(cfg: &ProviderConfig, keys: &Keys) -> Option<String> {
    for k in cfg.env_keys() {
        if let Some(v) = keys.env.get(&k).filter(|v| !v.trim().is_empty()) {
            return Some(v.clone());
        }
    }
    keys.store.get(&cfg.account())
}

/// True when a run can ask this provider without asking the person for anything.
pub fn ready(cfg: &ProviderConfig, keys: &Keys) -> bool {
    !cfg.needs_key || key_for(cfg, keys).is_some()
}

/// Check a key (or a keyless server) with one model list. Ok with the count.
pub fn check(cfg: &ProviderConfig, key: Option<&str>) -> Result<usize, String> {
    list_models(cfg, key, LIST_TIMEOUT).map(|m| m.len()).map_err(|e| list_error(cfg, &e))
}

// ---- Seats as heads ----

/// What a provider seat carries beside its head.
#[derive(Debug, Clone)]
struct Seat {
    head: &'static Head,
    cfg: ProviderConfig,
    model: String,
}

/// The providers `find` reads. None until `use_configs` runs; then the saved settings.
static CONFIGS: RwLock<Option<Vec<ProviderConfig>>> = RwLock::new(None);
/// One head per seat id and address, made once per process.
static SEATS: Mutex<Vec<Seat>> = Mutex::new(Vec::new());

/// The providers this process uses (the saved settings, or a test's).
pub fn use_configs(list: &[ProviderConfig]) {
    *CONFIGS.write().unwrap_or_else(|e| e.into_inner()) = Some(list.to_vec());
}

/// Add or replace one provider in this process's list (tests, and a run that adds one).
pub fn remember(cfg: &ProviderConfig) {
    let mut g = CONFIGS.write().unwrap_or_else(|e| e.into_inner());
    let list = g.get_or_insert_with(|| crate::settings::Settings::load().providers);
    list.retain(|c| c.id != cfg.id);
    list.push(cfg.clone());
}

/// The providers this process uses.
pub fn configs() -> Vec<ProviderConfig> {
    if let Some(l) = CONFIGS.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return l.clone();
    }
    let l = crate::settings::Settings::load().providers;
    use_configs(&l);
    l
}

/// The provider for an id: an added one, else a preset with its default address.
pub fn config(id: &str) -> Option<ProviderConfig> {
    let low = id.trim().to_ascii_lowercase();
    configs().into_iter().find(|c| c.id == low).or_else(|| preset(&low).filter(|p| !p.base_url.is_empty()).map(ProviderConfig::from_preset))
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The head for one model of one provider. Made once per process, then reused.
pub fn head_for(cfg: &ProviderConfig, model: &str) -> &'static Head {
    let id = seat_id(&cfg.id, model);
    let mut seats = SEATS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = seats.iter().find(|s| s.head.id == id && s.cfg.base_url == cfg.base_url && s.cfg.name == cfg.name && s.cfg.needs_key == cfg.needs_key) {
        return s.head;
    }
    let env: Vec<&'static str> = cfg.env_keys().into_iter().map(leak).collect();
    let head: &'static Head = Box::leak(Box::new(Head {
        id: leak(id),
        provider: leak(cfg.account()),
        display: leak(model.to_string()),
        company: leak(cfg.name.clone()),
        emoji: if cfg.local() { "\u{1f4bb}" } else { "\u{1f310}" },
        env_keys: Box::leak(env.into_boxed_slice()),
        model_env: "",
        default_model: leak(model.to_string()),
        url_env: "",
        default_url: leak(format!("{}/chat/completions", normalize_base(&cfg.base_url))),
        max_tokens: MAX_TOKENS,
        kind: Kind::Provider,
        key_page: cfg.key_page(),
        in_app: false,
    }));
    seats.push(Seat { head, cfg: cfg.clone(), model: model.to_string() });
    head
}

/// The head for a seat id such as `ollama:llama3.2`. None when the provider is unknown.
pub fn head(id: &str) -> Option<&'static Head> {
    let (p, m) = split_seat(id)?;
    let cfg = config(p)?;
    Some(head_for(&cfg, m))
}

/// The provider and model behind a provider seat's head.
pub fn seat_of(head: &Head) -> Option<(ProviderConfig, String)> {
    if head.kind != Kind::Provider {
        return None;
    }
    let seats = SEATS.lock().unwrap_or_else(|e| e.into_inner());
    seats.iter().find(|s| std::ptr::eq(s.head, head) || s.head.id == head.id).map(|s| (s.cfg.clone(), s.model.clone()))
}

/// Every ticked seat of every added provider, in the order they were added.
pub fn ticked(list: &[ProviderConfig]) -> Vec<&'static Head> {
    list.iter().flat_map(|c| c.models.iter().map(move |m| head_for(c, &m.id))).collect()
}

/// Ticked seats a run can ask now (local ones, and keyed ones that have a key).
pub fn ticked_ready(list: &[ProviderConfig], keys: &Keys) -> Vec<&'static Head> {
    list.iter().filter(|c| ready(c, keys)).flat_map(|c| c.models.iter().map(move |m| head_for(c, &m.id))).collect()
}

/// The family a head belongs to, for mixing providers on a suggested panel:
/// the provider id on a provider seat, else the head id.
pub fn family(head: &Head) -> String {
    match split_seat(head.id) {
        Some((p, _)) if head.kind == Kind::Provider => p.to_string(),
        _ => head.id.to_string(),
    }
}

/// The transcript label: model and provider.
pub fn label(head: &Head) -> String {
    format!("{} {} ({})", head.emoji, head.display, head.company)
}

/// Ask a provider seat. The key is read the usual way; a keyless provider is asked without one.
pub fn ask(head: &Head, prompt: &str, timeout: Duration, keys: &Keys) -> (Outcome, String) {
    let Some((cfg, model)) = seat_of(head) else {
        return (Outcome::Missing(format!("{} is not an added provider.", head.id), Fix::AddKey), String::new());
    };
    let key = key_for(&cfg, keys);
    if cfg.needs_key && key.is_none() {
        return (Outcome::Missing(format!("Add a {} key in Settings, AI accounts.", cfg.name), Fix::AddKey), cfg.name.clone());
    }
    (chat(&cfg, &model, key.as_deref(), prompt, timeout), cfg.name.clone())
}

// ---- Found on this computer ----

/// A local provider found running, the way AI apps are found.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    pub preset: String,
    pub name: String,
    pub base_url: String,
    pub models: Vec<ModelInfo>,
    /// Always "free, on this computer".
    pub note: String,
}

pub const LOCAL_NOTE: &str = "free, on this computer";

/// Where to look. `ROUNDTABLE_LOCAL_PROVIDERS` ("ollama=http://..,lmstudio=http://..") replaces it, for tests.
fn local_places() -> Vec<(&'static Preset, String)> {
    if let Ok(list) = std::env::var("ROUNDTABLE_LOCAL_PROVIDERS") {
        return list
            .split(',')
            .filter_map(|p| p.split_once('='))
            .filter_map(|(id, u)| preset(id).map(|p| (p, u.trim().to_string())))
            .collect();
    }
    PRESETS.iter().filter(|p| !p.base_url.is_empty() && is_local_url(p.base_url)).map(|p| (p, p.base_url.to_string())).collect()
}

/// Ollama and LM Studio running on this computer, with their models. Sends no prompt.
pub fn detect_local() -> Vec<Found> {
    let places = local_places();
    let found: Vec<Option<Found>> = std::thread::scope(|s| {
        let hs: Vec<_> = places
            .iter()
            .map(|(p, url)| {
                s.spawn(move || {
                    let mut cfg = ProviderConfig::from_preset(p);
                    cfg.base_url = normalize_base(url);
                    list_models(&cfg, None, PROBE_TIMEOUT).ok().map(|models| Found { preset: p.id.into(), name: p.name.into(), base_url: cfg.base_url.clone(), models, note: LOCAL_NOTE.into() })
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().ok().flatten()).collect()
    });
    found.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests;
