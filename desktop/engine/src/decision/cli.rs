//! `roundtable decision ...` and the decision-model keys in `roundtable auth`.

use super::{decide, options, provider, Outcome, Verdict};
use crate::keys::{self, KeyStore, Keys};
use crate::settings::Settings;
use serde_json::json;
use std::io::Read;

pub const USAGE: &str = "usage: roundtable decision {status|detect|test [jev|jebadiah|systemone]} [--json]";

/// `roundtable auth set|remove jev|jebadiah|systemone`. None when `provider` is not a decision model.
pub fn auth(sub: &str, provider: Option<&str>, store: &dyn KeyStore, json_out: bool) -> Option<u8> {
    let id = super::parse_provider(provider?).filter(|p| *p != "off")?;
    let (_, account) = provider::key_names(id);
    let reply = |v: serde_json::Value, code: u8| -> u8 {
        if json_out {
            println!("{v}");
        } else {
            println!("{}", v["message"].as_str().unwrap_or(""));
        }
        code
    };
    Some(match sub {
        "set" => {
            let mut raw = String::new();
            let _ = std::io::stdin().read_to_string(&mut raw);
            let key = keys::clean_key(&raw);
            if key.is_empty() {
                return Some(reply(json!({"ok": false, "message": "No key was pasted."}), 2));
            }
            match store.set(account, &key) {
                Ok(()) => reply(json!({"ok": true, "head": id, "message": "Saved."}), 0),
                Err(e) => reply(json!({"ok": false, "reason": "keychain", "message": format!("Could not save the key: {e}")}), 4),
            }
        }
        "remove" => match store.remove(account) {
            Ok(()) => reply(json!({"ok": true, "head": id, "message": "Removed."}), 0),
            Err(e) => reply(json!({"ok": false, "message": e}), 4),
        },
        _ => return None,
    })
}

/// The saved URL and model belong to the saved provider only.
fn scoped(id: &str, settings: &Settings) -> super::DecisionSettings {
    if settings.decision.active() == Some(id) {
        settings.decision.clone()
    } else {
        super::DecisionSettings { name: settings.decision.name.clone(), ..Default::default() }
    }
}

/// Status of every decision provider: chosen, key present (never the key), local Jebadiah.
pub fn status(keys: &Keys, settings: &Settings) -> serde_json::Value {
    let det = super::detect::find_jebadiah(keys, provider::key_for("jebadiah", keys).as_deref());
    let rows: Vec<_> = super::PROVIDERS
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "name": provider::display_name(id, &scoped(id, settings)),
                "has_key": provider::key_for(id, keys).is_some(),
                "ready": provider::resolve(id, &scoped(id, settings), keys).is_ok(),
            })
        })
        .collect();
    json!({"active": settings.decision.active(), "settings": settings.decision, "providers": rows, "jebadiah": det})
}

/// One sample decision through the provider, to say whether it works.
pub fn test(id: &str, keys: &Keys, settings: &Settings) -> (bool, serde_json::Value) {
    let positions = vec![
        ("Seat 1".to_string(), "POSITION: Bake bread on Saturday, because rain is forecast all day.".to_string()),
        ("Seat 2".to_string(), "POSITION: Plant the garden on Saturday; the soil is ready now.".to_string()),
    ];
    let ds = scoped(id, settings);
    let v = Verdict { question: "Saturday is forecast to rain all day. Bake bread or plant the garden?", context: None, positions: &positions, chair: None, consensus: None };
    let model = match provider::resolve(id, &ds, keys) {
        Ok(m) => m,
        Err(u) => return (false, json!({"ok": false, "provider": u.provider, "message": u.message})),
    };
    struct Seats;
    impl options::OptionSource for Seats {
        fn options(&self, v: &Verdict) -> options::OptionSet {
            options::from_positions(v)
        }
    }
    let t0 = std::time::Instant::now();
    let out = decide(&v, model.as_ref(), &Seats);
    let secs = (t0.elapsed().as_secs_f64() * 100.0).round() / 100.0;
    match out {
        Outcome::Rated(r) => (true, json!({"ok": true, "provider": r.provider, "endpoint": r.endpoint, "model": r.model, "line": r.line, "top": r.top, "latency_ms": r.latency_ms, "secs": secs})),
        Outcome::Unavailable(u) => (false, json!({"ok": false, "provider": u.provider, "endpoint": model.endpoint(), "message": u.message, "secs": secs})),
    }
}

pub fn run(args: &[String]) -> u8 {
    let json_out = args.iter().any(|a| a == "--json");
    let settings = Settings::load();
    let store = keys::default_store();
    let k = Keys::load(store.as_ref());
    let print = |v: &serde_json::Value| {
        if json_out {
            println!("{v}");
        } else {
            println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
        }
    };
    match args.first().map(String::as_str).unwrap_or("status") {
        "status" => {
            print(&status(&k, &settings));
            0
        }
        "detect" => {
            print(&json!(super::detect::find_jebadiah(&k, provider::key_for("jebadiah", &k).as_deref())));
            0
        }
        "test" => {
            let id = args.get(1).filter(|a| !a.starts_with("--")).and_then(|a| super::parse_provider(a)).or(settings.decision.active().and_then(super::parse_provider));
            let Some(id) = id.filter(|i| *i != "off") else {
                eprintln!("{USAGE}");
                return 2;
            };
            let (ok, v) = test(id, &k, &settings);
            if json_out {
                println!("{v}");
            } else if ok {
                println!("{} at {} ({}): {}", v["provider"].as_str().unwrap_or(""), v["endpoint"].as_str().unwrap_or(""), v["model"].as_str().unwrap_or(""), v["line"].as_str().unwrap_or(""));
            } else {
                println!("{}: {}", v["provider"].as_str().unwrap_or(""), v["message"].as_str().unwrap_or(""));
            }
            if ok {
                0
            } else {
                1
            }
        }
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}
