use super::*;
use crate::keys::KeyStore;
use serde_json::json;

fn cfg(id: &str, base: &str, needs_key: bool) -> ProviderConfig {
    ProviderConfig { id: id.into(), preset: "custom".into(), name: id.to_uppercase(), base_url: base.into(), needs_key, models: Vec::new() }
}

#[test]
fn presets_and_ids() {
    assert_eq!(preset("OpenRouter").unwrap().base_url, "https://openrouter.ai/api/v1");
    assert_eq!(preset("lm-studio").unwrap().id, "lmstudio");
    assert_eq!(preset("LM Studio").unwrap().id, "lmstudio");
    assert!(!preset("ollama").unwrap().needs_key);
    assert!(preset("groq").unwrap().needs_key);
    assert!(preset("bogus").is_none());
    assert_eq!(make_id("My Proxy!"), "my-proxy");
    assert_eq!(make_id("  "), "custom");
}

#[test]
fn seat_ids_split_at_the_first_colon() {
    assert_eq!(split_seat("ollama:llama3.2:latest"), Some(("ollama", "llama3.2:latest")));
    assert_eq!(split_seat("openrouter:meta-llama/llama-3.3-70b-instruct:free"), Some(("openrouter", "meta-llama/llama-3.3-70b-instruct:free")));
    assert_eq!(split_seat("claude"), None);
    assert_eq!(split_seat("ollama:"), None);
    assert_eq!(split_seat("a/b:c"), None);
}

#[test]
fn base_urls() {
    assert_eq!(normalize_base("http://localhost:11434"), "http://localhost:11434/v1");
    assert_eq!(normalize_base("http://localhost:11434/"), "http://localhost:11434/v1");
    assert_eq!(normalize_base("https://proxy.example.com/v1/chat/completions"), "https://proxy.example.com/v1");
    assert_eq!(normalize_base("https://openrouter.ai/api/v1/"), "https://openrouter.ai/api/v1");
    assert_eq!(normalize_base("localhost:1234/v1/models"), "http://localhost:1234/v1");
}

#[test]
fn local_addresses() {
    for u in ["http://localhost:11434/v1", "http://127.0.0.1:1234/v1", "http://[::1]:8080/v1", "http://0.0.0.0:8000", "http://app.localhost/v1"] {
        assert!(is_local_url(u), "{u}");
    }
    for u in ["https://openrouter.ai/api/v1", "http://192.168.1.4:11434/v1", "https://localhost.example.com/v1", "http://user@evil.com/127.0.0.1"] {
        assert!(!is_local_url(u), "{u}");
    }
}

#[test]
fn parses_openai_openrouter_ollama_and_together_lists() {
    let openai = json!({"object": "list", "data": [{"id": "gpt-4o-mini", "object": "model"}, {"id": "gpt-4o-mini"}]});
    let m = parse_models(&openai);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].id, "gpt-4o-mini");
    assert_eq!(m[0].price_in, None);

    let openrouter = json!({"data": [
        {"id": "openai/gpt-4o-mini", "name": "OpenAI: GPT-4o-mini", "context_length": 128000, "pricing": {"prompt": "0.00000015", "completion": "0.0000006"}},
        {"id": "openrouter/auto", "name": "Auto Router", "pricing": {"prompt": "-1", "completion": "-1"}}
    ]});
    let m = parse_models(&openrouter);
    assert!((m[0].price_in.unwrap() - 0.15).abs() < 1e-9);
    assert!((m[0].price_out.unwrap() - 0.6).abs() < 1e-9);
    assert_eq!(m[0].context, Some(128000));
    assert_eq!(m[0].name.as_deref(), Some("OpenAI: GPT-4o-mini"));
    assert_eq!(m[1].price_in, None, "a router model has no published price");

    let ollama = json!({"models": [{"name": "llama3.2:latest", "model": "llama3.2:latest"}, {"name": "qwen3:4b"}]});
    assert_eq!(parse_models(&ollama).iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["llama3.2:latest", "qwen3:4b"]);

    let together = json!([{"id": "meta-llama/Llama-3.3-70B-Instruct-Turbo", "display_name": "Llama 3.3 70B", "pricing": {"input": 0.88, "output": 0.88}}]);
    let m = parse_models(&together);
    assert_eq!(m[0].price_in, Some(0.88));
    assert_eq!(m[0].name.as_deref(), Some("Llama 3.3 70B"));

    assert!(parse_models(&json!({"error": "nope"})).is_empty());
}

#[test]
fn only_frontier_infra_jebadiah_is_ours() {
    let v = json!({"data": [{"id": "frontier-infra/jebadiah-9b"}, {"id": "hf.co/frontier-infra/jebadiah-27b:q4"}, {"id": "someone/jebadiah-7b"}, {"id": "jebadiah"}, {"id": "llama3.2"}]});
    let m = parse_models(&v);
    assert!(m[0].ours && m[1].ours);
    assert!(!m[2].ours && !m[3].ours);
    assert!(m[2].note.as_deref().unwrap().contains("Not a frontier-infra build"));
    assert!(m[3].note.as_deref().unwrap().contains("Not a frontier-infra build"));
    assert!(m[4].note.is_none() && !m[4].ours);
}

#[test]
fn search_matches_every_word() {
    let m = parse_models(&json!({"data": [{"id": "openai/gpt-4o-mini", "name": "GPT-4o mini"}, {"id": "meta-llama/llama-3.3-70b"}, {"id": "qwen/qwen3-8b"}]}));
    assert_eq!(search(&m, "").len(), 3);
    assert_eq!(search(&m, "LLAMA 70b").iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["meta-llama/llama-3.3-70b"]);
    assert_eq!(search(&m, "mini").len(), 1);
    assert!(search(&m, "claude").is_empty());
}

#[test]
fn seat_body_is_plain_chat() {
    let b = seat_body("m", "Q");
    assert_eq!(b["messages"], json!([{"role": "user", "content": "Q"}]));
    assert_eq!(b["stream"], false);
    for k in ["tools", "tool_choice", "functions", "function_call", "response_format", "plugins", "web_search_options"] {
        assert!(b.get(k).is_none(), "{k} must not be sent to a seat");
    }
    assert!(b.as_object().unwrap().keys().all(|k| ["model", "messages", "max_tokens", "temperature", "stream"].contains(&k.as_str())));
}

#[test]
fn heads_for_seats_carry_model_and_provider() {
    let mut c = cfg("unit-local", "http://127.0.0.1:9/v1", false);
    c.name = "Ollama".into();
    remember(&c);
    let h = head("unit-local:llama3.2:latest").unwrap();
    assert_eq!(h.id, "unit-local:llama3.2:latest");
    assert_eq!(h.kind, Kind::Provider);
    assert_eq!(h.provider, "provider-unit-local");
    assert_eq!(label(h), "\u{1f4bb} llama3.2:latest (Ollama)");
    assert_eq!(h.default_url, "http://127.0.0.1:9/v1/chat/completions");
    assert!(std::ptr::eq(h, head("unit-local:llama3.2:latest").unwrap()), "one head per seat");
    assert_eq!(family(h), "unit-local");
    let (got, model) = seat_of(h).unwrap();
    assert_eq!((got.id.as_str(), model.as_str()), ("unit-local", "llama3.2:latest"));
    assert!(head("nobody-added-this:m").is_none());
    // A keyless preset works without adding it first.
    let o = head("ollama:qwen3:4b").unwrap();
    assert_eq!(o.company, "Ollama");
}

#[test]
fn keys_env_then_keychain() {
    let store = crate::keys::MemoryStore::default();
    let mut c = cfg("unit-or", "https://openrouter.ai/api/v1", true);
    c.preset = "openrouter".into();
    let k = Keys { store: &store, env: Default::default(), file: Default::default() };
    assert!(!ready(&c, &k));
    store.set("provider-unit-or", "kc").unwrap();
    assert_eq!(key_for(&c, &k).as_deref(), Some("kc"));
    let env = [("OPENROUTER_API_KEY".to_string(), "env".to_string())].into_iter().collect();
    let k = Keys { store: &store, env, file: Default::default() };
    assert_eq!(key_for(&c, &k).as_deref(), Some("env"));
    assert!(ready(&c, &k));
    assert_eq!(c.env_keys(), ["OPENROUTER_API_KEY", "ROUNDTABLE_PROVIDER_UNIT_OR_KEY"]);
}

#[test]
fn settings_keep_providers_and_no_key() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("settings.json");
    let mut s = crate::settings::Settings::default();
    let mut c = cfg("or", "https://openrouter.ai/api/v1", true);
    c.models.push(SeatModel { id: "openai/gpt-4o-mini".into(), price_in: Some(0.15), price_out: Some(0.6) });
    s.providers.push(c);
    s.save_to(&p).unwrap();
    assert_eq!(crate::settings::Settings::load_from(&p), s);
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("openai/gpt-4o-mini") && !text.to_ascii_lowercase().contains("\"key\""));
    // An older settings file without providers still loads.
    std::fs::write(&p, "{\"history\": false}").unwrap();
    assert!(crate::settings::Settings::load_from(&p).providers.is_empty());
}
