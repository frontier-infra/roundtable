//! Provider seats against a fake OpenAI-compatible server: the model list (and Ollama's
//! `/api/tags`), the chat call, errors, a timeout and a rate limit, then the real binary:
//! `provider add|list|models|remove` and `ask --seats provider:model`.

use roundtable_engine::council::Outcome;
use roundtable_engine::custom::{self, ProviderConfig};
use roundtable_engine::http::ErrorClass;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the fake does for one request.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Ok,
    /// 429 on the first chat call, then answers.
    BusyOnce,
    /// 429 every time.
    AlwaysBusy,
    /// 401 on everything.
    BadKey,
    /// Waits two seconds before answering.
    Slow,
    /// Lists models only at Ollama's /api/tags.
    TagsOnly,
}

/// (method, path, authorization header, body) per request.
type Seen = Arc<Mutex<Vec<(String, String, Option<String>, String)>>>;

struct Fake {
    base: String,
    seen: Seen,
}

fn fake(mode: Mode) -> Fake {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        let mut chats = 0;
        for stream in l.incoming().flatten() {
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            if r.read_line(&mut first).unwrap_or(0) == 0 {
                continue;
            }
            let mut parts = first.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            let (mut len, mut auth) = (0usize, None);
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                let low = line.to_ascii_lowercase();
                if let Some(v) = low.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
                if low.starts_with("authorization:") {
                    auth = Some(line["authorization:".len()..].trim().to_string());
                }
            }
            let mut buf = vec![0; len];
            let _ = r.read_exact(&mut buf);
            let body = String::from_utf8_lossy(&buf).to_string();
            log.lock().unwrap().push((method.clone(), path.clone(), auth, body.clone()));
            let (status, out) = match (mode, method.as_str(), path.as_str()) {
                (Mode::BadKey, ..) => (401, json!({"error": {"message": "Invalid API key"}})),
                (Mode::TagsOnly, "GET", "/v1/models") => (404, json!({"error": "not found"})),
                (_, "GET", "/v1/models") => (200, json!({"data": [{"id": "small-a", "pricing": {"prompt": "0.000001", "completion": "0.000002"}}, {"id": "small-b"}, {"id": "someone/jebadiah-7b"}]})),
                (_, "GET", "/api/tags") => (200, json!({"models": [{"name": "small-a"}, {"name": "tagged-c:latest"}]})),
                (_, "POST", "/v1/chat/completions") => {
                    chats += 1;
                    let v: Value = serde_json::from_str(&body).unwrap_or(json!({}));
                    let model = v["model"].as_str().unwrap_or("").to_string();
                    match mode {
                        Mode::AlwaysBusy => (429, json!({"error": {"message": "rate limit"}})),
                        Mode::BusyOnce if chats == 1 => (429, json!({"error": {"message": "rate limit"}})),
                        Mode::Slow => {
                            std::thread::sleep(Duration::from_secs(2));
                            (200, json!({"choices": [{"message": {"content": "late"}}]}))
                        }
                        // Spends a 4000-token budget thinking; answers when given more room.
                        _ if model == "thinker" && v["max_tokens"].as_u64().unwrap_or(0) <= 4000 => (200, json!({"choices": [{"message": {"content": "", "reasoning_content": "long thought"}, "finish_reason": "length"}]})),
                        _ if model == "always-thinking" => (200, json!({"choices": [{"message": {"content": "<think>still going"}, "finish_reason": "length"}]})),
                        _ if model == "empty" => (200, json!({"choices": [{"message": {"content": "  "}}]})),
                        _ if model == "gone" => (404, json!({"error": {"message": "model not found"}})),
                        _ => (200, json!({"choices": [{"message": {"content": format!("<think>x</think>answer from {model}")}}]})),
                    }
                }
                _ => (404, json!({"error": "no route"})),
            };
            let text = out.to_string();
            let mut s = stream;
            let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
        }
    });
    Fake { base: format!("http://{addr}/v1"), seen }
}

fn cfg(id: &str, base: &str, needs_key: bool) -> ProviderConfig {
    ProviderConfig { id: id.into(), preset: "custom".into(), name: "Fake".into(), base_url: base.into(), needs_key, models: Vec::new() }
}

// ---- The client ----

#[test]
fn lists_models_with_prices_and_the_key() {
    let f = fake(Mode::Ok);
    let m = custom::list_models(&cfg("x", &f.base, true), Some("sk-test"), Duration::from_secs(5)).unwrap();
    assert_eq!(m.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["small-a", "small-b", "someone/jebadiah-7b"]);
    // A server on this computer is free, whatever it publishes (custom::tests covers the prices).
    assert!(m[0].free, "a 127.0.0.1 server is on this computer");
    assert_eq!((m[0].price_in, m[0].price_out), (Some(0.0), Some(0.0)));
    assert!(!m[2].ours, "a Jebadiah from another account is not ours");
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen[0].1, "/v1/models");
    assert_eq!(seen[0].2.as_deref(), Some("Bearer sk-test"));
}

#[test]
fn ollama_also_reads_api_tags() {
    let f = fake(Mode::TagsOnly);
    let mut c = cfg("ollama", &f.base, false);
    c.preset = "ollama".into();
    let m = custom::list_models(&c, None, Duration::from_secs(5)).unwrap();
    assert_eq!(m.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["small-a", "tagged-c:latest"]);
    assert!(f.seen.lock().unwrap().iter().all(|s| s.2.is_none()), "no key header without a key");
}

#[test]
fn list_errors_are_plain() {
    let f = fake(Mode::BadKey);
    let c = cfg("x", &f.base, true);
    let e = custom::list_models(&c, Some("bad"), Duration::from_secs(5)).unwrap_err();
    assert_eq!(e.class, ErrorClass::BadKey);
    assert!(custom::list_error(&c, &e).contains("needs a working key"));
    // Nothing listening.
    let dead = cfg("y", "http://127.0.0.1:9/v1", false);
    let e = custom::list_models(&dead, None, Duration::from_secs(2)).unwrap_err();
    assert!(custom::list_error(&dead, &e).contains("Is it running?"));
}

#[test]
fn chat_is_one_plain_user_message() {
    let f = fake(Mode::Ok);
    let o = custom::chat(&cfg("x", &f.base, false), "small-a", None, "Q?", Duration::from_secs(5));
    assert_eq!(o, Outcome::Answered("answer from small-a".into()));
    let seen = f.seen.lock().unwrap();
    let (m, p, auth, body) = &seen[0];
    assert_eq!((m.as_str(), p.as_str()), ("POST", "/v1/chat/completions"));
    assert!(auth.is_none());
    let v: Value = serde_json::from_str(body).unwrap();
    assert_eq!(v["messages"], json!([{"role": "user", "content": "Q?"}]));
    assert!(v.get("tools").is_none() && v.get("tool_choice").is_none() && v.get("functions").is_none());
}

#[test]
fn chat_errors() {
    let f = fake(Mode::Ok);
    let c = cfg("x", &f.base, false);
    assert!(matches!(custom::chat(&c, "empty", None, "Q", Duration::from_secs(5)), Outcome::Empty(_)));
    match custom::chat(&c, "gone", None, "Q", Duration::from_secs(5)) {
        Outcome::ToolFailed(_, m) => assert!(m.contains("does not have gone")),
        o => panic!("{o:?}"),
    }
    let f = fake(Mode::BadKey);
    assert!(matches!(custom::chat(&cfg("x", &f.base, true), "m", Some("bad"), "Q", Duration::from_secs(5)), Outcome::Failed(ErrorClass::BadKey, _)));
    assert!(matches!(custom::chat(&cfg("x", &f.base, false), "m", None, "Q", Duration::from_secs(5)), Outcome::Missing(..)));
    match custom::chat(&cfg("x", "http://127.0.0.1:9/v1", false), "m", None, "Q", Duration::from_secs(2)) {
        Outcome::ToolFailed(_, m) => assert!(m.contains("not answering on this computer")),
        o => panic!("{o:?}"),
    }
}

#[test]
fn a_thinking_model_out_of_room_is_asked_once_more_with_more_room() {
    let f = fake(Mode::Ok);
    let c = cfg("x", &f.base, false);
    assert_eq!(custom::chat(&c, "thinker", None, "Q", Duration::from_secs(5)), Outcome::Answered("answer from thinker".into()));
    {
        let seen = f.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        let budgets: Vec<u64> = seen.iter().map(|s| serde_json::from_str::<Value>(&s.3).unwrap()["max_tokens"].as_u64().unwrap()).collect();
        assert_eq!(budgets, vec![custom::MAX_TOKENS as u64, custom::THINKING_MAX_TOKENS as u64]);
    }
    match custom::chat(&c, "always-thinking", None, "Q", Duration::from_secs(5)) {
        Outcome::Empty(m) => assert!(m.contains("whole answer budget thinking"), "{m}"),
        o => panic!("{o:?}"),
    }
    assert_eq!(f.seen.lock().unwrap().len(), 4, "only once more");
    // A plain empty answer (not out of room) is not asked again.
    assert!(matches!(custom::chat(&c, "empty", None, "Q", Duration::from_secs(5)), Outcome::Empty(_)));
    assert_eq!(f.seen.lock().unwrap().len(), 5);
}

#[test]
fn chat_times_out() {
    let f = fake(Mode::Slow);
    assert_eq!(custom::chat(&cfg("x", &f.base, false), "m", None, "Q", Duration::from_millis(500)), Outcome::Timeout);
}

#[test]
fn a_rate_limit_is_asked_once_more() {
    std::env::set_var("ROUNDTABLE_RATE_LIMIT_WAIT_MS", "50");
    let f = fake(Mode::BusyOnce);
    assert_eq!(custom::chat(&cfg("x", &f.base, false), "m", None, "Q", Duration::from_secs(5)), Outcome::Answered("answer from m".into()));
    assert_eq!(f.seen.lock().unwrap().len(), 2);
    let f = fake(Mode::AlwaysBusy);
    let o = custom::chat(&cfg("x", &f.base, false), "m", None, "Q", Duration::from_secs(5));
    assert_eq!(o.reason(), Some(ErrorClass::RateLimited));
    assert_eq!(f.seen.lock().unwrap().len(), 2, "only once more");
}

// ---- The binary ----

fn engine(data: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_roundtable-engine"));
    c.env("ROUNDTABLE_DATA_DIR", data)
        .env("ROUNDTABLE_KEYSTORE", "memory")
        .env("HOME", data)
        .env("USERPROFILE", data)
        .env("ROUNDTABLE_TOOL_PATH", "")
        .env("ROUNDTABLE_NO_SHELL_PATH", "1")
        .env("ROUNDTABLE_LOCAL_PROVIDERS", "")
        .env("ROUNDTABLE_SUGGEST", "words")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENROUTER_API_KEY");
    c
}

fn run_json(c: &mut Command) -> Value {
    let o = c.output().unwrap();
    serde_json::from_slice(&o.stdout).unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&o.stdout)))
}

#[test]
fn cli_add_list_models_ask_remove() {
    let d = tempfile::tempdir().unwrap();
    let f = fake(Mode::Ok);
    // add, with ticked models; prices come from the list.
    let v = run_json(engine(d.path()).args(["provider", "add", "custom", "--name", "Fake Proxy", "--url", &f.base, "--models", "small-a,small-b", "--json"]));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["seats"], json!(["fake-proxy:small-a", "fake-proxy:small-b"]));
    assert_eq!(v["provider"]["local"], true);
    let v = run_json(engine(d.path()).args(["provider", "list", "--json"]));
    assert_eq!(v["providers"][0]["models"][0]["price_in"], 0.0, "local seats are free");
    assert_eq!(v["providers"][0]["note"], "free, on this computer");
    assert_eq!(v["presets"].as_array().unwrap().len(), 6);
    // models, with a search.
    let v = run_json(engine(d.path()).args(["provider", "models", "fake-proxy", "--search", "SMALL", "--json"]));
    assert_eq!(v["total"], 3);
    assert_eq!(v["models"].as_array().unwrap().len(), 2);
    assert_eq!(v["ticked"], json!(["small-a", "small-b"]));
    // ask with --seats: two seats, plain chat calls, labels name the model and the provider.
    let o = engine(d.path()).args(["ask", "-q", "Pick one?", "--seats", "fake-proxy:small-a,fake-proxy:small-b", "--no-history", "--no-clarify", "--verdict", "none"]).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}\n{}", String::from_utf8_lossy(&o.stderr));
    assert!(out.contains("## \u{1f4bb} small-a (Fake Proxy)\n\nanswer from small-a"), "{out}");
    assert!(out.contains("## \u{1f4bb} small-b (Fake Proxy)\n\nanswer from small-b"), "{out}");
    let chats: Vec<Value> = f.seen.lock().unwrap().iter().filter(|s| s.1.ends_with("/chat/completions")).map(|s| serde_json::from_str(&s.3).unwrap()).collect();
    assert_eq!(chats.len(), 2);
    assert!(chats.iter().all(|c| c.get("tools").is_none() && c["messages"].as_array().unwrap().len() == 1));
    // test, the way a card's Test button runs it.
    let v = run_json(engine(d.path()).args(["test", "fake-proxy:small-a", "--json"]));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["via"], "Fake Proxy");
    // remove.
    let v = run_json(engine(d.path()).args(["provider", "remove", "fake-proxy", "--json"]));
    assert_eq!(v["ok"], true);
    let v = run_json(engine(d.path()).args(["provider", "list", "--json"]));
    assert!(v["providers"].as_array().unwrap().is_empty());
}

#[test]
fn cli_key_goes_to_the_keychain_never_to_settings_or_output() {
    let d = tempfile::tempdir().unwrap();
    let f = fake(Mode::Ok);
    let marker = "sk-PROVIDER-LEAKCHECK-42";
    let mut c = engine(d.path());
    c.args(["provider", "add", "custom", "--name", "Keyed", "--url", &f.base, "--key-stdin", "--check", "--json"]).stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(marker.as_bytes()).unwrap();
    let o = child.wait_with_output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{out}");
    assert!(!out.contains(marker));
    // The check sent it as a bearer key.
    assert!(f.seen.lock().unwrap().iter().any(|s| s.2.as_deref() == Some(&format!("Bearer {marker}"))));
    let settings = std::fs::read_to_string(d.path().join("settings.json")).unwrap();
    assert!(!settings.contains(marker));
    assert!(settings.contains("\"needs_key\": true"));
}

#[test]
fn cli_errors() {
    let d = tempfile::tempdir().unwrap();
    let v = run_json(engine(d.path()).args(["provider", "add", "custom", "--json"]));
    assert_eq!(v["ok"], false);
    assert!(v["message"].as_str().unwrap().contains("base URL"));
    let v = run_json(engine(d.path()).args(["provider", "add", "nope", "--json"]));
    assert!(v["message"].as_str().unwrap().contains("Unknown provider"));
    let v = run_json(engine(d.path()).args(["provider", "models", "openrouter", "--json"]));
    assert_eq!(v["reason"], "add_key");
    let f = fake(Mode::BadKey);
    let mut c = engine(d.path());
    c.args(["provider", "add", "custom", "--name", "Bad", "--url", &f.base, "--key-stdin", "--check", "--json"]).stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"sk-bad").unwrap();
    let v: Value = serde_json::from_slice(&child.wait_with_output().unwrap().stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["reason"], "bad_key");
    let v = run_json(engine(d.path()).args(["provider", "list", "--json"]));
    assert!(v["providers"].as_array().unwrap().is_empty(), "a failed check saves nothing");
}

#[test]
fn cli_detects_local_servers() {
    let d = tempfile::tempdir().unwrap();
    let f = fake(Mode::TagsOnly);
    let root = f.base.trim_end_matches("/v1");
    let v = run_json(engine(d.path()).env("ROUNDTABLE_LOCAL_PROVIDERS", format!("ollama={root},lmstudio=http://127.0.0.1:9")).args(["provider", "detect", "--json"]));
    let found = v["found"].as_array().unwrap();
    assert_eq!(found.len(), 1, "{v}");
    assert_eq!(found[0]["name"], "Ollama");
    assert_eq!(found[0]["note"], "free, on this computer");
    assert_eq!(found[0]["models"][1]["id"], "tagged-c:latest");
}

#[test]
fn ticked_seats_sit_by_default_and_personas_mix_providers() {
    let d = tempfile::tempdir().unwrap();
    let f = fake(Mode::Ok);
    run_json(engine(d.path()).args(["provider", "add", "custom", "--name", "Fake", "--url", &f.base, "--models", "small-a,small-b", "--json"]));
    // No --heads and no keys: the ticked seats are the table.
    let o = engine(d.path()).args(["ask", "-q", "Pick?", "--no-history", "--no-clarify", "--verdict", "none"]).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("small-a (Fake)") && out.contains("small-b (Fake)"), "{out}");
    // A suggested panel spreads seats over providers: with two Fake seats and the
    // Ollama preset seat, the first two personas sit on different providers.
    let v = run_json(engine(d.path()).args(["suggest", "-q", "Should we raise prices? Budget, revenue and hiring.", "--heads", "fake:small-a,fake:small-b,ollama:tiny", "--json"]));
    let heads: Vec<&str> = v["seats"].as_array().unwrap().iter().map(|s| s["head"].as_str().unwrap()).collect();
    assert!(heads.len() >= 2, "{v}");
    assert_ne!(heads[0].split(':').next(), heads[1].split(':').next(), "{heads:?}");
}

#[test]
fn mcp_takes_seats() {
    let d = tempfile::tempdir().unwrap();
    let f = fake(Mode::Ok);
    run_json(engine(d.path()).args(["provider", "add", "custom", "--name", "Fake", "--url", &f.base, "--json"]));
    let call = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "roundtable", "arguments": {"question": "Pick?", "seats": "fake:small-b", "verdict": "none"}}});
    let mut c = engine(d.path());
    c.args(["mcp", "serve"]).stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(format!("{call}\n").as_bytes()).unwrap();
    let o = child.wait_with_output().unwrap();
    let line = String::from_utf8_lossy(&o.stdout);
    let v: Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("small-b (Fake)") && text.contains("answer from small-b"), "{text}");
}

#[test]
fn a_ticked_seat_chairs_when_claude_cannot_be_asked() {
    let d = tempfile::tempdir().unwrap();
    let f = fake(Mode::Ok);
    run_json(engine(d.path()).args(["provider", "add", "custom", "--name", "Fake", "--url", &f.base, "--models", "small-a,small-b", "--json"]));
    let o = engine(d.path()).args(["ask", "-q", "Pick?", "--rounds", "2", "--no-history", "--no-clarify", "--verdict", "none"]).output().unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("### \u{1fa91} Chair verdict - round 1\n\nanswer from small-a"), "{out}");
    assert!(!out.contains("chair unavailable"), "{out}");
    // Round 2 read round 1 and the chair's reading.
    let bodies: Vec<String> = f.seen.lock().unwrap().iter().filter(|s| s.1.ends_with("/chat/completions")).map(|s| s.3.clone()).collect();
    assert!(bodies.iter().any(|b| b.contains("COUNCIL DELIBERATION") && b.contains("THE CHAIR'S READING OF ROUND 1")));
}
