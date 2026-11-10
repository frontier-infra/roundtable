use super::*;
use crate::council::{self, AskOptions};
use crate::keys::{KeyStore, MemoryStore};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// Fake decision model: fixed probabilities, records what it was asked.
struct FakeModel {
    probs: Vec<(&'static str, f64)>,
    seen: Mutex<Option<(Value, Vec<TypedOption>)>>,
}

impl FakeModel {
    fn new(probs: Vec<(&'static str, f64)>) -> FakeModel {
        FakeModel { probs, seen: Mutex::new(None) }
    }
}

impl DecisionModel for FakeModel {
    fn name(&self) -> &str {
        "Fake"
    }
    fn endpoint(&self) -> &str {
        "fake://"
    }
    fn choose(&self, state: &Value, _i: &str, options: &[TypedOption]) -> Result<Answer, String> {
        *self.seen.lock().unwrap() = Some((state.clone(), options.to_vec()));
        Ok(Answer { probabilities: self.probs.iter().map(|(k, p)| (k.to_string(), *p)).collect(), confidence: Some(0.5), model: "fake-1".into(), latency_ms: Some(5) })
    }
}

struct FailModel;
impl DecisionModel for FailModel {
    fn name(&self) -> &str {
        "Jev"
    }
    fn endpoint(&self) -> &str {
        "fake://"
    }
    fn choose(&self, _: &Value, _: &str, _: &[TypedOption]) -> Result<Answer, String> {
        Err("That key did not work. Check you copied all of it.".into())
    }
}

/// Fake option source: the chair's list.
struct Fixed(Vec<&'static str>, Option<usize>);
impl OptionSource for Fixed {
    fn options(&self, _: &Verdict) -> options::OptionSet {
        options::build(&self.0.iter().map(|s| s.to_string()).collect::<Vec<_>>(), self.1)
    }
}

fn table() -> Vec<(String, String)> {
    vec![
        ("Claude".into(), "POSITION: **Use Postgres.** It handles the joins.".into()),
        ("GPT".into(), "POSITION:\nUse SQLite, it is one file.".into()),
        ("Grok".into(), "_(unavailable, skipped)_".into()),
    ]
}

#[test]
fn line_reads_like_the_spec() {
    let t = table();
    let v = Verdict { question: "Which database?", context: None, positions: &t, chair: Some("VERDICT: CONSENSUS\n- Postgres"), consensus: Some(true) };
    let m = FakeModel::new(vec![("A", 0.62), ("B", 0.31), (MORE_INFO, 0.07)]);
    let Outcome::Rated(r) = decide(&v, &m, &Fixed(vec!["Use Postgres", "Use SQLite"], Some(0))) else { panic!("rated") };
    assert_eq!(r.line, "Decision model: Fake rates option A 0.62, B 0.31, more information 0.07. Confidence 0.50.");
    assert_eq!(r.top, "A");
    assert!(!r.disagrees);
    let md = Outcome::Rated(r).markdown();
    assert!(md.starts_with("### Decision model\n\nDecision model: Fake rates option A 0.62"));
    assert!(md.contains("- **Option B** (0.31): Use SQLite\n"));
    assert!(md.contains("- **More information** (0.07): Need more information"));
    assert!(!md.contains('\u{2014}'), "no em dashes");
    // The model read the question, every position and the chair.
    let (state, opts) = m.seen.lock().unwrap().clone().unwrap();
    assert_eq!(state["question"], "Which database?");
    assert_eq!(state["positions"].as_array().unwrap().len(), 3);
    assert_eq!(state["chair_declared_consensus"], true);
    assert_eq!(opts.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(), vec!["A", "B", MORE_INFO]);
}

#[test]
fn disagreement_is_said_plainly() {
    let t = table();
    let v = Verdict { question: "Which database?", context: None, positions: &t, chair: Some("VERDICT: CONSENSUS"), consensus: Some(true) };
    let m = FakeModel::new(vec![("A", 0.2), ("B", 0.7), (MORE_INFO, 0.1)]);
    let Outcome::Rated(r) = decide(&v, &m, &Fixed(vec!["Use Postgres", "Use SQLite"], Some(0))) else { panic!() };
    assert!(r.disagrees);
    assert_eq!(r.disagreement.as_deref(), Some("The decision model disagrees with the table's consensus: the table chose option A, but Fake rates option B highest (0.70)."));
    assert!(Outcome::Rated(r).markdown().contains("**The decision model disagrees with the table's consensus"));

    let m = FakeModel::new(vec![("A", 0.2), ("B", 0.1), (MORE_INFO, 0.7)]);
    let Outcome::Rated(r) = decide(&v, &m, &Fixed(vec!["Use Postgres", "Use SQLite"], Some(0))) else { panic!() };
    assert!(r.disagreement.unwrap().contains("says more information is needed first (0.70)"));
}

#[test]
fn probabilities_are_normalised_and_missing_ones_are_zero() {
    let t = table();
    let v = Verdict { question: "Q", context: None, positions: &t, chair: None, consensus: None };
    let m = FakeModel::new(vec![("A", 2.0), ("B", 2.0)]);
    let Outcome::Rated(r) = decide(&v, &m, &Fixed(vec!["x", "y"], None)) else { panic!() };
    assert_eq!(r.probabilities, vec![0.5, 0.5, 0.0]);
    assert_eq!(r.top, "A", "ties go to the first option");
    assert!(!r.disagrees, "no table pick, nothing to disagree with");
}

#[test]
fn a_failed_call_is_one_plain_line() {
    let t = table();
    let v = Verdict { question: "Q", context: None, positions: &t, chair: None, consensus: None };
    let o = decide(&v, &FailModel, &Fixed(vec!["x", "y"], None));
    assert_eq!(o.markdown(), "### Decision model\n\nDecision model: Jev did not answer. That key did not work. Check you copied all of it.\n");
}

#[test]
fn seats_opening_lines_are_the_fallback_options() {
    let t = table();
    let v = Verdict { question: "Q", context: None, positions: &t, chair: None, consensus: None };
    let set = options::from_positions(&v);
    let texts: Vec<_> = set.options.iter().map(|o| o.text.as_str()).collect();
    assert_eq!(texts[0], "Use Postgres. It handles the joins.");
    assert_eq!(texts[1], "Use SQLite, it is one file.");
    assert_eq!(set.options.len(), 3, "skipped seat dropped, more information added");
}

#[test]
fn chair_reply_parsing() {
    let set = options::parse_reply("Here:\n```json\n{\"options\": [\"Postgres\", \"SQLite\", \"postgres\"], \"table_pick\": 1}\n```").unwrap();
    assert_eq!(set.options.len(), 3, "duplicates fold");
    assert_eq!(set.table_pick.as_deref(), Some("B"));
    assert!(options::parse_reply("no json").is_none());
    assert!(options::parse_reply("{\"options\": []}").is_none());
}

#[test]
fn flag_overrides_settings() {
    let s = with_override(Settings::default(), Some("jebadiah")).unwrap();
    assert_eq!(s.decision.active(), Some("jebadiah"));
    let s = with_override(s, Some("off")).unwrap();
    assert_eq!(s.decision.active(), None);
    let s = with_override(Settings::default(), Some("link21")).unwrap();
    assert_eq!((s.decision.active(), s.decision.name.as_deref()), (Some("systemone"), Some("LINK21")));
    assert!(with_override(Settings::default(), Some("bogus")).is_err());
    assert_eq!(Settings::default().decision.active(), None, "off by default");
}

#[test]
fn jev_without_a_key_says_so() {
    let store = MemoryStore::default();
    let k = Keys { store: &store, env: BTreeMap::new(), file: BTreeMap::new() };
    let mut s = Settings::default();
    s.decision.provider = Some("jev".into());
    let t = table();
    let v = Verdict { question: "Q", context: None, positions: &t, chair: None, consensus: None };
    let o = after_verdict(&v, &k, &s, std::time::Duration::from_secs(1)).unwrap();
    assert!(matches!(&o, Outcome::Unavailable(u) if u.message.starts_with("Add your Jev key")));
    assert!(after_verdict(&v, &k, &Settings::default(), std::time::Duration::from_secs(1)).is_none(), "off means no call");
}

/// Tiny HTTP server: answers by path, records each POST body.
#[allow(clippy::type_complexity)]
fn server(routes: Vec<(&'static str, u16, String)>) -> (String, Arc<Mutex<Vec<(String, String, Value)>>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        for stream in l.incoming().flatten() {
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            let _ = r.read_line(&mut first);
            let path = first.split_whitespace().nth(1).unwrap_or("").to_string();
            let (mut len, mut auth) = (0usize, String::new());
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
                    auth = line["authorization:".len()..].trim().to_string();
                }
            }
            let mut buf = vec![0; len];
            let _ = r.read_exact(&mut buf);
            seen2.lock().unwrap().push((path.clone(), auth, serde_json::from_slice(&buf).unwrap_or(Value::Null)));
            let (status, body) = routes.iter().find(|(p, _, _)| path.starts_with(p)).map(|(_, s, b)| (*s, b.clone())).unwrap_or((404, "{}".into()));
            let mut s = stream;
            let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    (format!("http://{addr}"), seen)
}

fn systemone_reply() -> String {
    json!({"model": "frontier-infra/jebadiah-9b-v2", "answers": {"best_option": {"type": "choice", "choice": "A", "confidence": 0.42, "probabilities": {"A": 0.61, "B": 0.2, "more_information": 0.19}}}, "latency_ms": 192.2}).to_string()
}

#[test]
fn system_one_wire_and_local_detection() {
    let (base, seen) = server(vec![
        ("/v1/models", 200, json!({"object": "list", "data": [{"id": "frontier-infra/jebadiah-4b-v2"}, {"id": "frontier-infra/jebadiah-9b-v2"}, {"id": "whisper"}]}).to_string()),
        ("/v1/systemone", 200, systemone_reply()),
    ]);
    let store = MemoryStore::default();
    store.set("jebadiah", "local-key").unwrap();
    let k = Keys { store: &store, env: [("ROUNDTABLE_JEBADIAH_PROBE".to_string(), format!("http://127.0.0.1:9,{base}"))].into(), file: BTreeMap::new() };
    let d = detect::find_jebadiah(&k, Some("local-key"));
    let f = d.found.clone().expect("found");
    assert_eq!((f.url.as_str(), f.model.as_str()), (base.as_str(), "frontier-infra/jebadiah-9b-v2"));
    assert_eq!(d.looked.len(), 2, "the empty port is listed too");

    let mut s = Settings::default();
    s.decision.provider = Some("jebadiah".into());
    let m = provider::resolve("jebadiah", &s.decision, &k).ok().unwrap();
    assert_eq!(m.endpoint(), format!("{base}/v1/systemone"));
    let t = table();
    let v = Verdict { question: "Which database?", context: Some("We run on one VM."), positions: &t, chair: None, consensus: None };
    let Outcome::Rated(r) = decide(&v, m.as_ref(), &Fixed(vec!["Use Postgres", "Use SQLite"], None)) else { panic!("rated") };
    assert_eq!(r.line, "Decision model: Jebadiah rates option A 0.61, B 0.20, more information 0.19. Confidence 0.42.");
    assert_eq!(r.latency_ms, Some(192));
    let calls = seen.lock().unwrap();
    let (_, auth, body) = calls.iter().find(|(p, _, _)| p == "/v1/systemone").unwrap();
    assert_eq!(auth, "Bearer local-key");
    assert_eq!(body["model"], "frontier-infra/jebadiah-9b-v2");
    assert_eq!(body["state"]["context"], "We run on one VM.");
    let q = &body["questions"]["best_option"];
    assert_eq!(q["type"], "choice");
    assert_eq!(q["criteria"].as_object().unwrap().keys().cloned().collect::<Vec<_>>(), vec!["A", "B", MORE_INFO], "criteria keep their order");
    assert!(!format!("{:?}", r).contains("local-key"), "no key in the report");
}

#[test]
fn council_run_carries_the_decision_into_transcript_and_events() {
    // Other tests point ROUNDTABLE_GLM_URL at their own servers; this lock keeps them apart.
    let _env = crate::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("ROUNDTABLE_TOOL_PATH", "");
    let (heads_base, _) = server(vec![("/v1/chat/completions", 200, json!({"choices": [{"message": {"content": "POSITION: Bake bread."}}]}).to_string())]);
    let (s1, seen) = server(vec![("/v1/systemone", 200, systemone_reply())]);
    std::env::set_var("ROUNDTABLE_GLM_URL", format!("{heads_base}/v1/chat/completions"));
    let store = MemoryStore::default();
    let k = Keys { store: &store, env: [("ZAI_API_KEY".to_string(), "test".to_string())].into(), file: BTreeMap::new() };
    let s = Settings {
        decision: DecisionSettings { provider: Some("systemone".into()), url: Some(s1.clone()), model: Some("link21-test".into()), name: Some("LINK21".into()) },
        ..Default::default()
    };
    let opts = AskOptions { question: "Bread or garden?".into(), context: None, heads: council::parse_heads("glm,minimax").0, seats: Vec::new(), rounds: 1, research: false, timeout: std::time::Duration::from_secs(10), ..Default::default() };
    let events = Mutex::new(Vec::new());
    let r = council::run(&opts, &k, &s, &|e| events.lock().unwrap().push(serde_json::to_value(e).unwrap()));
    std::env::remove_var("ROUNDTABLE_GLM_URL");
    // No chair could be asked in the test, so the seats' opening lines are the options: one answer, plus more information.
    assert!(r.transcript.contains("### Decision model\n\nDecision model: LINK21 rates option A "), "{}", r.transcript);
    assert!(r.transcript.contains("- **Option A** "));
    assert!(matches!(r.decision, Some(Outcome::Rated(_))));
    let ev = events.lock().unwrap();
    let d = ev.iter().find(|e| e["event"] == "decision").expect("decision event");
    assert_eq!(d["status"], "rated");
    assert_eq!(d["provider"], "LINK21");
    let body = &seen.lock().unwrap()[0].2;
    assert_eq!(body["model"], "link21-test");
    assert_eq!(body["state"]["positions"][0]["position"], "POSITION: Bake bread.");
}

#[test]
fn only_our_jebadiah_builds_count() {
    let list: Value = serde_json::from_str(include_str!("jebadiah-ids.json")).unwrap();
    let strs = |k: &str| list[k].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(strs("namespaces"), detect::OURS, "OURS matches jebadiah-ids.json");
    assert_eq!(strs("hosts"), detect::HOSTS, "HOSTS matches jebadiah-ids.json");
    for id in strs("ours") {
        assert!(detect::is_ours(&id), "{id} is ours");
    }
    for id in strs("not_ours") {
        assert!(!detect::is_ours(&id), "{id} is not ours");
    }
    let ids: Vec<String> = ["hf.co/Jebadiah/jebadiah-9b-x:Q4", "frontierinfra/jebadiah:4b"].iter().map(|s| s.to_string()).collect();
    assert_eq!(detect::pick(&ids).as_deref(), Some("frontierinfra/jebadiah:4b"), "a stranger's 9b never beats our 4b");
}

#[test]
fn detect_skips_a_strangers_jebadiah_in_ollama_and_on_a_server() {
    // A fake Ollama list with both kinds, and a server that lists only a stranger's model.
    let (ollama, _) = server(vec![(
        "/api/tags",
        200,
        json!({"models": [{"name": "hf.co/Jebadiah/jebadiah-9b-uncensored-GGUF:Q4_K_M"}, {"name": "frontierinfra/jebadiah:9b"}, {"name": "llama3.2:3b"}]}).to_string(),
    )]);
    let (stranger, _) = server(vec![("/v1/models", 200, json!({"object": "list", "data": [{"id": "Jebadiah/some-adult-model"}]}).to_string())]);
    let store = MemoryStore::default();
    let env = |weights: String| -> BTreeMap<String, String> {
        [("ROUNDTABLE_JEBADIAH_PROBE".to_string(), stranger.clone()), ("ROUNDTABLE_JEBADIAH_WEIGHTS".to_string(), weights)].into()
    };
    let k = Keys { store: &store, env: env(format!("Ollama={ollama}/api/tags")), file: BTreeMap::new() };
    let d = detect::find_jebadiah(&k, None);
    assert_eq!(d.found, None, "a stranger's server is not used as the decision model");
    assert_eq!(d.looked[0], format!("{stranger}: answered, no Jebadiah model"));
    assert_eq!(d.looked[1], "Ollama: 3 models");
    assert_eq!(d.hint.as_deref(), Some("Ollama has Jebadiah. Run `jeb serve` (pip install jebadiah-decide) so Roundtable can ask it."));
    assert_eq!(
        d.note.as_deref(),
        Some("Skipped 2 unrelated models named Jebadiah, not frontier-infra builds: Jebadiah/some-adult-model, hf.co/Jebadiah/jebadiah-9b-uncensored-GGUF:Q4_K_M.")
    );
    let out = serde_json::to_value(&d).unwrap();
    assert!(out["note"].as_str().unwrap().starts_with("Skipped 2 unrelated"), "detect output carries the note");

    // Ollama with only the stranger's model: no hint to run it, one-line note.
    let (only_stranger, _) = server(vec![("/api/tags", 200, json!({"models": [{"name": "hf.co/Jebadiah/jebadiah-9b-uncensored-GGUF:Q4_K_M"}]}).to_string())]);
    let (empty, _) = server(vec![("/v1/models", 200, json!({"object": "list", "data": []}).to_string())]);
    let k = Keys {
        store: &store,
        env: [("ROUNDTABLE_JEBADIAH_PROBE".to_string(), empty.clone()), ("ROUNDTABLE_JEBADIAH_WEIGHTS".to_string(), format!("Ollama={only_stranger}/api/tags"))].into(),
        file: BTreeMap::new(),
    };
    let d = detect::find_jebadiah(&k, None);
    assert_eq!((d.found, d.hint), (None, None));
    assert_eq!(d.note.as_deref(), Some("Skipped hf.co/Jebadiah/jebadiah-9b-uncensored-GGUF:Q4_K_M: an unrelated model named Jebadiah, not a frontier-infra build."));

    // Our build behind a server is found; nothing to note.
    let (ours, _) = server(vec![("/v1/models", 200, json!({"object": "list", "data": [{"id": "frontier-infra/jebadiah-9b-v2"}]}).to_string())]);
    let k = Keys { store: &store, env: [("ROUNDTABLE_JEBADIAH_PROBE".to_string(), format!("{stranger},{ours}"))].into(), file: BTreeMap::new() };
    let d = detect::find_jebadiah(&k, None);
    assert_eq!(d.found.map(|f| f.model).as_deref(), Some("frontier-infra/jebadiah-9b-v2"));
    assert_eq!(d.note.as_deref(), Some("Skipped Jebadiah/some-adult-model: an unrelated model named Jebadiah, not a frontier-infra build."));
}
