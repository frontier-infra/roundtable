//! The council: same flow and transcript as `lib/core.sh`.
//!
//! Advisory (`rounds <= 1`): each head answers once, blind.
//! Deliberation (`rounds >= 2`): round 1 blind; later rounds see the whole table;
//! a Claude chair says CONSENSUS or CONTINUE after each round and the run stops early on consensus.

use crate::http::{self, ErrorClass};
use crate::keys::Keys;
use crate::providers::{self, Head, Kind, HEADS};
use crate::settings::Settings;
use serde::Serialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AskOptions {
    pub question: String,
    /// Background text every head reads first.
    pub context: Option<String>,
    pub heads: Vec<&'static Head>,
    pub rounds: u32,
    pub research: bool,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Answered(String),
    /// No key or no local tool.
    Missing(String),
    Timeout,
    Empty(String),
    Failed(ErrorClass, String),
}

impl Outcome {
    /// The text the transcript shows for this head (same sentinels as `body_of_file`).
    pub fn body(&self) -> String {
        match self {
            Outcome::Answered(t) => t.clone(),
            Outcome::Missing(_) => "_(unavailable, skipped)_".into(),
            Outcome::Timeout => "_(timed out)_".into(),
            Outcome::Empty(_) => "_(no answer)_".into(),
            Outcome::Failed(c, _) => format!("_(no answer: {})_", c.message()),
        }
    }
    pub fn status(&self) -> &'static str {
        match self {
            Outcome::Answered(_) => "answered",
            Outcome::Missing(_) => "skipped",
            Outcome::Timeout => "timed_out",
            Outcome::Empty(_) => "failed",
            Outcome::Failed(..) => "failed",
        }
    }
    pub fn reason(&self) -> Option<ErrorClass> {
        match self {
            Outcome::Failed(c, _) => Some(*c),
            Outcome::Timeout => Some(ErrorClass::Timeout),
            _ => None,
        }
    }
}

/// One JSON line per event with `--events`. Drives the app's live cards.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    RunStarted { heads: Vec<String>, rounds: u32, mode: String },
    HeadStarted { head: String, round: u32 },
    HeadDone {
        head: String,
        round: u32,
        status: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<ErrorClass>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    RoundDone { round: u32 },
    ChairVerdict { round: u32, consensus: bool, text: String },
    RunDone {
        transcript: String,
        answered: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        consensus: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        history_id: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub transcript: String,
    pub answered: usize,
    pub consensus: Option<bool>,
}

/// Default per-head time limit: 300 s, or 360 s with web search.
pub fn default_timeout(research: bool, settings: &Settings) -> Duration {
    Duration::from_secs(settings.timeout_secs.unwrap_or(if research { 360 } else { 300 }))
}

/// Parse a comma list of heads. Unknown names come back as warnings.
pub fn parse_heads(csv: &str) -> (Vec<&'static Head>, Vec<String>) {
    let mut out = Vec::new();
    let mut warn = Vec::new();
    for name in csv.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match HEADS.iter().find(|h| h.id == name) {
            Some(h) if !out.iter().any(|o: &&Head| o.id == h.id) => out.push(h),
            Some(_) => {}
            None => warn.push(format!("WARN: unknown head '{name}'")),
        }
    }
    (out, warn)
}

/// Heads for a run with no `--heads`: settings, then ROUNDTABLE_HEADS, then every
/// head that can answer (has a key, or the local tool). Falls back to the full list.
pub fn default_heads(settings: &Settings, keys: &Keys) -> Vec<&'static Head> {
    if !settings.default_heads.is_empty() {
        let (h, _) = parse_heads(&settings.default_heads.join(","));
        if !h.is_empty() {
            return h;
        }
    }
    let env_heads = keys.env.get("ROUNDTABLE_HEADS").or_else(|| keys.file.get("ROUNDTABLE_HEADS"));
    if let Some(csv) = env_heads.filter(|v| !v.trim().is_empty()) {
        let (h, _) = parse_heads(csv);
        if !h.is_empty() {
            return h;
        }
    }
    let (all, _) = parse_heads(providers::DEFAULT_HEADS);
    let ready: Vec<_> = all
        .iter()
        .copied()
        .filter(|h| match h.kind {
            Kind::CodexCli => find_exe("codex").is_some(),
            _ => keys.key(h).is_some() || (h.id == "grok" && find_exe("grok").is_some()),
        })
        .collect();
    if ready.is_empty() {
        all
    } else {
        ready
    }
}

pub fn base_prompt(question: &str, context: Option<&str>) -> String {
    match context {
        Some(c) => format!("## CONTEXT\n{}\n\n## QUESTION\n{}", c.trim_end_matches('\n'), question),
        None => question.to_string(),
    }
}

fn deliberation_prompt(base: &str, prev: u32, table: &[(String, String)], round: u32) -> String {
    let mut p = format!("{base}\n\n---\n## COUNCIL DELIBERATION: every councilor's position from round {prev}:\n\n");
    for (label, body) in table {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    p.push_str(&format!(
        "## YOUR TASK (round {round})\nYou are ONE councilor. You have now read everyone. Update your position. State explicitly: (a) where a peer changed your mind; name them; (b) where you still disagree and why; (c) your sharpened current answer. Begin your reply with \"POSITION:\".\n"
    ));
    p
}

fn chair_prompt(question: &str, round: u32, table: &[(String, String)]) -> String {
    let mut p = format!(
        "You are the neutral CHAIR of an AI design council. The councilors submitted these round {round} positions to this question:\n\n{question}\n\n--- POSITIONS ---\n\n"
    );
    for (label, body) in table {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    p.push_str("--- YOUR JOB ---\nLine 1 must be EXACTLY one of: \"VERDICT: CONSENSUS\" (the councilors substantively agree on the core recommendation) or \"VERDICT: CONTINUE\" (material disagreement remains). Then give: **AGREED** (bullets) and **REMAINING DISAGREEMENTS** (bullets; name who holds what). Be strict: consensus means real convergence on the answer, not mere overlap.\n");
    p
}

/// True when line 1 reads `VERDICT: CONSENSUS` (any case, any spaces after the colon).
pub fn is_consensus(chair: &str) -> bool {
    let first = chair.lines().next().unwrap_or("").to_ascii_lowercase();
    match first.find("verdict:") {
        Some(i) => first[i + "verdict:".len()..].trim_start().starts_with("consensus"),
        None => false,
    }
}

/// Look for an executable on PATH and in the usual per-user folders.
pub fn find_exe(name: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        vec![".exe".into(), ".cmd".into(), ".bat".into(), String::new()]
    } else {
        vec![String::new()]
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home = crate::paths::home();
    for extra in [".local/bin", ".bun/bin", ".npm-global/bin", ".claude/local"] {
        dirs.push(home.join(extra));
    }
    if cfg!(target_os = "macos") {
        dirs.push("/opt/homebrew/bin".into());
        dirs.push("/usr/local/bin".into());
    }
    for d in dirs {
        for e in &exts {
            let p = d.join(format!("{name}{e}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Run a local tool with a time limit. Returns (exit code, stdout) or None on timeout.
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<Option<(i32, String)>, String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let mut out = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            use std::io::Read;
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => return Ok(Some((st.code().unwrap_or(-1), reader.join().unwrap_or_default()))),
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn openai_compat(head: &Head, key: &str, prompt: &str, research: bool, settings: &Settings, timeout: Duration) -> Outcome {
    let glm = if head.id == "glm" { Some(research) } else { None };
    let body = http::chat_body(&providers::model_for(head, settings), prompt, head.max_tokens, glm);
    match http::send("POST", &providers::url_for(head), &http::bearer(key), Some(&body), timeout) {
        Ok(v) => {
            let mut t = http::chat_text(&v);
            if head.id == "minimax" {
                t = http::strip_think(&t);
            }
            if t.is_empty() {
                Outcome::Empty(format!("{}: empty answer", head.id))
            } else {
                Outcome::Answered(t)
            }
        }
        Err(e) if e.class == ErrorClass::Timeout => Outcome::Timeout,
        Err(e) => Outcome::Failed(e.class, e.detail),
    }
}

/// Anthropic call for the Claude head and the chair.
pub fn anthropic(key: &str, model: &str, prompt: &str, timeout: Duration) -> Outcome {
    let url = providers::url_for(providers::find("claude").unwrap());
    let body = http::anthropic_body(model, prompt, 8000);
    match http::send("POST", &url, &http::anthropic_headers(key), Some(&body), timeout) {
        Ok(v) => {
            let t = http::anthropic_text(&v);
            if t.is_empty() {
                Outcome::Empty("claude: empty answer".into())
            } else {
                Outcome::Answered(t)
            }
        }
        Err(e) if e.class == ErrorClass::Timeout => Outcome::Timeout,
        Err(e) => Outcome::Failed(e.class, e.detail),
    }
}

fn grok_cli(prompt: &str, research: bool, timeout: Duration) -> Outcome {
    let Some(exe) = find_exe("grok") else {
        return Outcome::Missing("no XAI_API_KEY or grok CLI".into());
    };
    let model = std::env::var("ROUNDTABLE_GROK_CLI_MODEL").unwrap_or_else(|_| "grok-composer-2.5-fast".into());
    let mut c = Command::new(exe);
    c.args(["-p", prompt, "-m", &model, "--max-turns", if research { "8" } else { "6" }]);
    if !research {
        c.arg("--disable-web-search");
    }
    match run_with_timeout(c, timeout) {
        Ok(None) => Outcome::Timeout,
        Ok(Some((_, out))) if !out.trim().is_empty() => Outcome::Answered(out.trim().to_string()),
        Ok(Some((rc, _))) => Outcome::Empty(format!("grok cli rc={rc}")),
        Err(e) => Outcome::Empty(e),
    }
}

fn codex_cli(prompt: &str, research: bool, timeout: Duration) -> Outcome {
    let Some(exe) = find_exe("codex") else {
        return Outcome::Missing("no codex CLI".into());
    };
    let dir = std::env::temp_dir().join(format!("roundtable-codex-{}-{}", std::process::id(), nanos()));
    let _ = std::fs::create_dir_all(&dir);
    let last = dir.join("last.md");
    let mut c = Command::new(exe);
    c.args(["exec", "--skip-git-repo-check", "-s", "read-only"]);
    if !research {
        c.args(["-c", "tools.web_search=false"]);
    }
    c.arg("-o").arg(&last).arg(prompt);
    let r = run_with_timeout(c, timeout);
    let text = std::fs::read_to_string(&last).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    match r {
        Ok(None) => Outcome::Timeout,
        _ if !text.trim().is_empty() => Outcome::Answered(text.trim().to_string()),
        Ok(Some((rc, _))) => Outcome::Empty(format!("codex rc={rc}")),
        Err(e) => Outcome::Empty(e),
    }
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// Ask one head.
pub fn run_head(head: &Head, prompt: &str, opts: &AskOptions, keys: &Keys, settings: &Settings) -> Outcome {
    match head.kind {
        Kind::CodexCli => codex_cli(prompt, opts.research, opts.timeout),
        Kind::Anthropic => match keys.key(head) {
            Some(k) => anthropic(&k, &providers::model_for(head, settings), prompt, opts.timeout),
            None => Outcome::Missing("no ANTHROPIC_API_KEY".into()),
        },
        Kind::OpenAiCompat => match keys.key(head) {
            Some(k) => {
                let o = openai_compat(head, &k, prompt, opts.research, settings, opts.timeout);
                if head.id == "grok" && !matches!(o, Outcome::Answered(_)) && find_exe("grok").is_some() {
                    return grok_cli(prompt, opts.research, opts.timeout);
                }
                o
            }
            None if head.id == "grok" => grok_cli(prompt, opts.research, opts.timeout),
            None => Outcome::Missing(format!("no {}", head.env_keys.join("/"))),
        },
    }
}

fn run_round(prompt: &str, round: u32, opts: &AskOptions, keys: &Keys, settings: &Settings, emit: &(dyn Fn(&Event) + Sync)) -> Vec<Outcome> {
    std::thread::scope(|s| {
        let handles: Vec<_> = opts
            .heads
            .iter()
            .map(|h| {
                emit(&Event::HeadStarted { head: h.id.into(), round });
                s.spawn(move || {
                    let o = run_head(h, prompt, opts, keys, settings);
                    emit(&Event::HeadDone {
                        head: h.id.into(),
                        round,
                        status: o.status().into(),
                        text: if let Outcome::Answered(t) = &o { t.clone() } else { String::new() },
                        reason: o.reason(),
                        message: o.reason().map(|r| r.message().to_string()),
                    });
                    o
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or(Outcome::Empty("panic".into()))).collect()
    })
}

/// Run the council. `emit` gets every event as it happens; the transcript comes back.
pub fn run(opts: &AskOptions, keys: &Keys, settings: &Settings, emit: &(dyn Fn(&Event) + Sync)) -> RunResult {
    let n = opts.heads.len();
    let labels: Vec<String> = opts.heads.iter().map(|h| providers::label(h, settings)).collect();
    let advisory = opts.rounds <= 1;
    emit(&Event::RunStarted {
        heads: opts.heads.iter().map(|h| h.id.to_string()).collect(),
        rounds: opts.rounds.max(1),
        mode: if advisory { "advisory".into() } else { "deliberation".into() },
    });

    let mut out = String::new();
    let mut say = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    let mode = if advisory { "advisory".to_string() } else { format!("deliberation (\u{2264}{} rounds)", opts.rounds) };
    say(&format!("# \u{1fa91} Round Table - {n} heads - {mode}"));
    say("");
    say(&format!("_Question:_ {}", opts.question));
    say("");

    let base = base_prompt(&opts.question, opts.context.as_deref());
    let mut answered = 0usize;
    let mut consensus = None;

    if advisory {
        let res = run_round(&base, 1, opts, keys, settings, emit);
        emit(&Event::RoundDone { round: 1 });
        for (label, o) in labels.iter().zip(&res) {
            if matches!(o, Outcome::Answered(_)) {
                answered += 1;
            }
            say("---");
            say(&format!("## {label}"));
            say("");
            say(&o.body());
            say("");
        }
        say("---");
        say("_Advisory mode: independent answers, no cross-talk. Use `--rounds N` for deliberation to consensus._");
    } else {
        let chair_key = keys.key(providers::find("claude").unwrap());
        let chair_model = providers::chair_model(settings);
        let mut prev: Vec<(String, String)> = Vec::new();
        let mut cons = false;
        for r in 1..=opts.rounds {
            let prompt = if r == 1 { base.clone() } else { deliberation_prompt(&base, r - 1, &prev, r) };
            let res = run_round(&prompt, r, opts, keys, settings, emit);
            emit(&Event::RoundDone { round: r });
            answered = answered.max(res.iter().filter(|o| matches!(o, Outcome::Answered(_))).count());
            prev = labels.iter().cloned().zip(res.iter().map(|o| o.body())).collect();
            say(&format!("## \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} Round {r} \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}"));
            say("");
            for (label, body) in &prev {
                say(&format!("### {label}"));
                say("");
                say(body);
                say("");
            }
            let chair = match &chair_key {
                Some(k) => match anthropic(k, &chair_model, &chair_prompt(&opts.question, r, &prev), opts.timeout) {
                    Outcome::Answered(t) => t,
                    _ => String::new(),
                },
                None => String::new(),
            };
            let chair = if chair.is_empty() {
                "VERDICT: CONTINUE\n_(chair unavailable: ANTHROPIC_API_KEY missing or error)_".to_string()
            } else {
                chair
            };
            let c = is_consensus(&chair);
            emit(&Event::ChairVerdict { round: r, consensus: c, text: chair.clone() });
            say(&format!("### \u{1fa91} Chair verdict - round {r}"));
            say("");
            say(&chair);
            say("");
            if c {
                cons = true;
                say(&format!("**\u{2192} Consensus reached at round {r}.**"));
                say("");
                break;
            }
        }
        say("---");
        if cons {
            say("_Deliberation converged. The chair's final verdict above is the council consensus._");
        } else {
            say(&format!(
                "_Reached the round cap ({}) without full consensus. The chair's last verdict lists the remaining disagreements._",
                opts.rounds
            ));
        }
        consensus = Some(cons);
    }
    RunResult { transcript: out, answered, consensus }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::MemoryStore;
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    #[test]
    fn consensus_parsing() {
        assert!(is_consensus("VERDICT: CONSENSUS\n- a"));
        assert!(is_consensus("verdict:   consensus"));
        assert!(is_consensus("**VERDICT: CONSENSUS**"));
        assert!(!is_consensus("VERDICT: CONTINUE"));
        assert!(!is_consensus("intro\nVERDICT: CONSENSUS"));
    }

    #[test]
    fn heads_parsing() {
        let (h, w) = parse_heads("claude, gemini,bogus,claude");
        assert_eq!(h.iter().map(|h| h.id).collect::<Vec<_>>(), vec!["claude", "gemini"]);
        assert_eq!(w, vec!["WARN: unknown head 'bogus'"]);
    }

    #[test]
    fn context_prompt() {
        assert_eq!(base_prompt("Q", Some("ctx\n\n")), "## CONTEXT\nctx\n\n## QUESTION\nQ");
        assert_eq!(base_prompt("Q", None), "Q");
    }

    #[test]
    fn default_heads_prefers_settings_then_keys() {
        let store = MemoryStore::default();
        let mut s = Settings::default();
        s.default_heads = vec!["gemini".into()];
        let k = Keys { store: &store, env: BTreeMap::new(), file: BTreeMap::new() };
        assert_eq!(default_heads(&s, &k)[0].id, "gemini");
        let k = Keys {
            store: &store,
            env: [("ROUNDTABLE_HEADS".to_string(), "glm,openai".to_string())].into(),
            file: BTreeMap::new(),
        };
        assert_eq!(default_heads(&Settings::default(), &k).iter().map(|h| h.id).collect::<Vec<_>>(), vec!["glm", "openai"]);
    }

    /// Tiny fake provider: answers every chat call with a fixed text, or a status.
    fn fake_server(status: u16, body: &'static str) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in l.incoming().flatten() {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0usize;
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut buf = vec![0; len];
                let _ = r.read_exact(&mut buf);
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        format!("http://{addr}/v1/chat/completions")
    }

    #[test]
    fn advisory_run_against_fake_provider() {
        let url = fake_server(200, r#"{"choices":[{"message":{"content":"Bake bread."}}]}"#);
        std::env::set_var("ROUNDTABLE_GLM_URL", &url);
        let store = MemoryStore::default();
        let k = Keys {
            store: &store,
            env: [("ZAI_API_KEY".to_string(), "test".to_string())].into(),
            file: BTreeMap::new(),
        };
        let (heads, _) = parse_heads("glm,minimax");
        let opts = AskOptions {
            question: "Bread or garden?".into(),
            context: None,
            heads,
            rounds: 1,
            research: false,
            timeout: Duration::from_secs(10),
        };
        let events = Mutex::new(Vec::new());
        let r = run(&opts, &k, &Settings::default(), &|e| events.lock().unwrap().push(serde_json::to_value(e).unwrap()));
        std::env::remove_var("ROUNDTABLE_GLM_URL");
        assert_eq!(r.answered, 1);
        assert!(r.transcript.starts_with("# \u{1fa91} Round Table - 2 heads - advisory\n"));
        assert!(r.transcript.contains("## \u{1f7e3} GLM (glm-5.2, Z.AI)\n\nBake bread.\n"));
        assert!(r.transcript.contains("_(unavailable, skipped)_"));
        assert!(!r.transcript.contains('\u{2014}'), "no em dashes");
        let ev = events.lock().unwrap();
        assert_eq!(ev.first().unwrap()["event"], "run_started");
        let done: Vec<_> = ev.iter().filter(|e| e["event"] == "head_done").collect();
        assert_eq!(done.len(), 2);
        assert!(done.iter().any(|e| e["head"] == "minimax" && e["status"] == "skipped"));
    }

    #[test]
    fn bad_key_is_a_plain_failure() {
        let url = fake_server(401, r#"{"error":"invalid key"}"#);
        std::env::set_var("ROUNDTABLE_MINIMAX_URL", &url);
        let store = MemoryStore::default();
        let k = Keys { store: &store, env: [("MINIMAX_API_KEY".to_string(), "x".to_string())].into(), file: BTreeMap::new() };
        let opts = AskOptions {
            question: "Q".into(),
            context: None,
            heads: parse_heads("minimax").0,
            rounds: 2,
            research: false,
            timeout: Duration::from_secs(10),
        };
        let r = run(&opts, &k, &Settings::default(), &|_| {});
        std::env::remove_var("ROUNDTABLE_MINIMAX_URL");
        assert_eq!(r.answered, 0);
        assert_eq!(r.consensus, Some(false));
        assert!(r.transcript.contains("That key did not work"));
        assert!(r.transcript.contains("chair unavailable"));
        assert!(r.transcript.contains("Round 2"));
    }
}
