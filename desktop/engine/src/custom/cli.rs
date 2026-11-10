//! `roundtable provider add|list|models|remove|detect`: the command line and the app's
//! way to add a provider, list its models and tick the ones that sit at the table.
//! A key comes in on stdin only, goes straight to the keychain and is never printed.

use super::*;
use crate::keys::{self, KeyStore};
use crate::settings::Settings;
use serde_json::json;
use std::io::Read;

pub const USAGE: &str = "usage: roundtable provider <command> [--json]
  add <preset|custom> [--id ID] [--name NAME] [--url URL] [--key-stdin] [--models CSV] [--check]
        Add a provider or change one. Presets: openrouter, ollama, lmstudio, groq, together,
        custom (any OpenAI-compatible base URL). --key-stdin reads the key from stdin into the
        keychain. --models sets the ticked models: each one is a seat (provider:model).
  list  Added providers, their ticked seats and whether each has its key
  models <id> [--search WORDS]
        The models a provider lists (GET /v1/models; Ollama also /api/tags), with prices
        where the provider publishes them
  remove <id>
        Remove a provider, its seats and its saved key
  detect
        Ollama and LM Studio running on this computer (free, on this computer)";

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// The positional arguments, skipping flags and their values.
fn positionals(args: &[String]) -> Vec<&str> {
    const WITH_VALUE: &[&str] = &["--id", "--name", "--url", "--models", "--search", "--timeout"];
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if WITH_VALUE.contains(&a) {
            i += 2;
            continue;
        }
        if !a.starts_with("--") {
            out.push(a);
        }
        i += 1;
    }
    out
}

/// One provider as `list` shows it. Never a key.
pub fn row(cfg: &ProviderConfig, keys: &Keys) -> Value {
    let src = cfg
        .env_keys()
        .iter()
        .find(|k| keys.env.get(*k).is_some_and(|v| !v.trim().is_empty()))
        .map(|_| "env")
        .or_else(|| keys.store.get(&cfg.account()).map(|_| "keychain"));
    json!({
        "id": cfg.id,
        "preset": cfg.preset,
        "name": cfg.name,
        "base_url": normalize_base(&cfg.base_url),
        "needs_key": cfg.needs_key,
        "has_key": src.is_some(),
        "key_source": src,
        "local": cfg.local(),
        "note": if cfg.local() { Some(LOCAL_NOTE) } else { None },
        "key_page": cfg.key_page(),
        "ready": ready(cfg, keys),
        "models": cfg.models.iter().map(|m| json!({"id": m.id, "seat": seat_id(&cfg.id, &m.id), "price_in": m.price_in, "price_out": m.price_out})).collect::<Vec<_>>(),
    })
}

fn presets_json() -> Value {
    json!(PRESETS.iter().map(|p| json!({"id": p.id, "name": p.name, "base_url": p.base_url, "needs_key": p.needs_key, "local": !p.base_url.is_empty() && is_local_url(p.base_url), "key_page": p.key_page})).collect::<Vec<_>>())
}

/// Run `roundtable provider ...`. Returns the exit code.
pub fn run(args: &[String]) -> u8 {
    let store = keys::default_store();
    run_with(args, store.as_ref(), &crate::paths::settings_file(), &mut std::io::stdin())
}

/// Same, with the store, settings file and stdin given (tests).
pub fn run_with(args: &[String], store: &dyn KeyStore, settings_path: &std::path::Path, stdin: &mut dyn Read) -> u8 {
    let json_out = has(args, "--json");
    let reply = |v: Value, text: String, code: u8| -> u8 {
        if json_out {
            println!("{v}");
        } else {
            println!("{text}");
        }
        code
    };
    let sub = args.first().map(String::as_str).unwrap_or("list");
    let rest = if args.is_empty() { args } else { &args[1..] };
    let pos = positionals(rest);
    let mut settings = Settings::load_from(settings_path);
    use_configs(&settings.providers);
    let keys = Keys { store, env: std::env::vars().collect(), file: Default::default() };
    match sub {
        "list" => {
            let rows: Vec<Value> = settings.providers.iter().map(|c| row(c, &keys)).collect();
            let text = if settings.providers.is_empty() {
                "No providers added. Try: roundtable provider detect".to_string()
            } else {
                settings
                    .providers
                    .iter()
                    .map(|c| {
                        let state = if c.local() { LOCAL_NOTE.to_string() } else if ready(c, &keys) { "key saved".into() } else { "needs a key".into() };
                        let seats = if c.models.is_empty() { "no models ticked".to_string() } else { c.seat_ids().join(", ") };
                        format!("{:<12} {:<40} {state}\n  seats: {seats}", c.id, normalize_base(&c.base_url))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            reply(json!({"providers": rows, "presets": presets_json()}), text, 0)
        }
        "detect" => {
            let found = detect_local();
            let text = if found.is_empty() {
                "Neither Ollama nor LM Studio is running on this computer.".to_string()
            } else {
                found.iter().map(|f| format!("{:<10} {:<32} {} models, {}", f.name, f.base_url, f.models.len(), f.note)).collect::<Vec<_>>().join("\n")
            };
            reply(json!({"found": found}), text, 0)
        }
        "models" => {
            let Some(id) = pos.first() else {
                eprintln!("{USAGE}");
                return 2;
            };
            let Some(cfg) = settings.providers.iter().find(|c| c.id == id.to_ascii_lowercase()).cloned().or_else(|| config(id)) else {
                return reply(json!({"ok": false, "message": format!("No provider '{id}'. Add it first: roundtable provider add {id}")}), format!("No provider '{id}'. Add it first: roundtable provider add {id}"), 2);
            };
            let key = key_for(&cfg, &keys);
            if cfg.needs_key && key.is_none() {
                let m = format!("{} needs a key. Run: roundtable provider add {} --key-stdin", cfg.name, cfg.id);
                return reply(json!({"ok": false, "reason": "add_key", "message": m}), m, 3);
            }
            match list_models(&cfg, key.as_deref(), LIST_TIMEOUT) {
                Ok(all) => {
                    let q = flag(rest, "--search").unwrap_or("");
                    let shown: Vec<&ModelInfo> = search(&all, q);
                    let ticked: Vec<&str> = cfg.models.iter().map(|m| m.id.as_str()).collect();
                    let text = shown
                        .iter()
                        .map(|m| {
                            let price = match (m.free, m.price_in, m.price_out) {
                                (true, ..) => LOCAL_NOTE.to_string(),
                                (_, Some(i), Some(o)) => format!("${i:.2} in, ${o:.2} out per million tokens"),
                                _ => "price unknown".into(),
                            };
                            let tick = if ticked.contains(&m.id.as_str()) { "[x]" } else { "[ ]" };
                            let note = m.note.as_deref().map(|n| format!("  ({n})")).unwrap_or_default();
                            format!("{tick} {}  {price}{note}", m.id)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    reply(json!({"ok": true, "provider": cfg.id, "local": cfg.local(), "total": all.len(), "models": shown, "ticked": ticked}), text, 0)
                }
                Err(e) => {
                    let m = list_error(&cfg, &e);
                    reply(json!({"ok": false, "reason": e.class, "message": m}), m, 1)
                }
            }
        }
        "add" => {
            let Some(kind) = pos.first() else {
                eprintln!("{USAGE}");
                return 2;
            };
            let Some(p) = preset(kind) else {
                let m = format!("Unknown provider '{kind}'. Use openrouter, ollama, lmstudio, groq, together or custom.");
                return reply(json!({"ok": false, "message": m}), m, 2);
            };
            let id = flag(rest, "--id").map(make_id).unwrap_or_else(|| if p.id == "custom" { make_id(flag(rest, "--name").unwrap_or("custom")) } else { p.id.to_string() });
            let mut cfg = settings.providers.iter().find(|c| c.id == id).cloned().unwrap_or_else(|| {
                let mut c = ProviderConfig::from_preset(p);
                c.id = id.clone();
                c
            });
            if let Some(n) = flag(rest, "--name").filter(|n| !n.trim().is_empty()) {
                cfg.name = n.trim().to_string();
            }
            if let Some(u) = flag(rest, "--url").filter(|u| !u.trim().is_empty()) {
                cfg.base_url = normalize_base(u);
            }
            if cfg.base_url.trim().is_empty() {
                let m = "A custom provider needs its base URL: --url https://host/v1".to_string();
                return reply(json!({"ok": false, "message": m}), m, 2);
            }
            cfg.base_url = normalize_base(&cfg.base_url);
            let mut new_key = None;
            if has(rest, "--key-stdin") {
                let mut raw = String::new();
                let _ = stdin.read_to_string(&mut raw);
                let k = keys::clean_key(&raw);
                if !k.is_empty() {
                    new_key = Some(k);
                }
            }
            // A custom provider needs a key when the person gives one.
            if p.id == "custom" && new_key.is_some() {
                cfg.needs_key = true;
            }
            let key = new_key.clone().or_else(|| key_for(&cfg, &keys));
            let mut listed: Option<Vec<ModelInfo>> = None;
            if has(rest, "--check") || flag(rest, "--models").is_some() {
                match list_models(&cfg, key.as_deref(), LIST_TIMEOUT) {
                    Ok(l) => listed = Some(l),
                    Err(e) if has(rest, "--check") => {
                        let m = list_error(&cfg, &e);
                        return reply(json!({"ok": false, "reason": e.class, "message": m}), m, 3);
                    }
                    Err(_) => {}
                }
            }
            if let Some(csv) = flag(rest, "--models") {
                cfg.models = csv
                    .split(',')
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .fold(Vec::<SeatModel>::new(), |mut v, m| {
                        if !v.iter().any(|x| x.id == m) {
                            let info = listed.as_ref().and_then(|l| l.iter().find(|x| x.id == m));
                            v.push(SeatModel { id: m.to_string(), price_in: info.and_then(|i| i.price_in), price_out: info.and_then(|i| i.price_out) });
                        }
                        v
                    });
            }
            if let Some(k) = &new_key {
                if let Err(e) = store.set(&cfg.account(), k) {
                    let m = format!("Could not save the key: {e}");
                    return reply(json!({"ok": false, "reason": "keychain", "message": m}), m, 4);
                }
            }
            // Seats that are no longer ticked leave the saved table.
            let old: Vec<String> = settings.providers.iter().filter(|c| c.id == cfg.id).flat_map(|c| c.seat_ids()).collect();
            let now = cfg.seat_ids();
            settings.default_heads.retain(|h| !old.contains(h) || now.contains(h));
            settings.providers.retain(|c| c.id != cfg.id);
            settings.providers.push(cfg.clone());
            if let Err(e) = settings.save_to(settings_path) {
                let m = format!("Could not save settings: {e}");
                return reply(json!({"ok": false, "message": m}), m, 4);
            }
            use_configs(&settings.providers);
            let seats = cfg.seat_ids();
            let text = if seats.is_empty() {
                format!("Saved {}. Tick models with: roundtable provider add {} --models a,b", cfg.name, cfg.preset)
            } else {
                format!("Saved {}. Seats: {}", cfg.name, seats.join(", "))
            };
            reply(json!({"ok": true, "provider": row(&cfg, &Keys { store, env: keys.env.clone(), file: Default::default() }), "seats": seats, "message": text}), text, 0)
        }
        "remove" => {
            let Some(id) = pos.first().map(|s| s.to_ascii_lowercase()) else {
                eprintln!("{USAGE}");
                return 2;
            };
            let Some(cfg) = settings.providers.iter().find(|c| c.id == id).cloned() else {
                let m = format!("No provider '{id}'.");
                return reply(json!({"ok": false, "message": m}), m, 2);
            };
            let gone = cfg.seat_ids();
            settings.providers.retain(|c| c.id != id);
            settings.default_heads.retain(|h| !gone.contains(h));
            if let Err(e) = settings.save_to(settings_path) {
                let m = format!("Could not save settings: {e}");
                return reply(json!({"ok": false, "message": m}), m, 4);
            }
            let _ = store.remove(&cfg.account());
            use_configs(&settings.providers);
            let m = format!("Removed {} and its {} seat{}.", cfg.name, gone.len(), if gone.len() == 1 { "" } else { "s" });
            reply(json!({"ok": true, "message": m}), m, 0)
        }
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}
