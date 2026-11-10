//! `roundtable`: the engine command line. Same `ask` flags as the bash CLI,
//! plus machine-readable modes for the desktop app.

use roundtable_engine::council::{self, AskOptions, Event, RunResult};
use roundtable_engine::keys::{self, KeyStore, Keys};
use roundtable_engine::settings::Settings;
use roundtable_engine::{history, mcp, paths, providers, verify, VERSION};
use serde_json::json;
use std::io::{Read, Write};
use std::process::ExitCode;

const USAGE: &str = "Roundtable: ask several AI models one question (a Frontier Infra project)

Usage:
  roundtable \"question\"               Ask the table (advisory)
  roundtable ask \"question\" [flags]   Same, explicit
  roundtable <command>

Commands:
  ask            Run the council
  auth           set <provider> (key on stdin) | remove <provider> | status | import
  doctor         Show which AIs have a key (--online also checks each key)
  mcp            serve | selftest | config
  version        Print the version

Ask flags:
  -q, --question TEXT   The question (also accepted as a positional)
  -c, --context FILE    Background text every AI reads first
      --heads CSV       Subset: claude,openai,grok,glm,minimax,gemini,codex
      --rounds N        N>=2: talk it through, up to N rounds (default 1)
      --timeout S       Seconds per AI
      --research        Let them search the web where supported
      --out FILE        Also save the transcript to FILE
      --events          Print one JSON event per line (for the app)
      --no-history      Do not keep this run in history

Docs: https://roundtable.sh";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &args[..] } else { &args[1..] };
    let code = match cmd {
        "" | "-h" | "--help" | "help" => {
            println!("{USAGE}");
            0
        }
        "version" | "--version" | "-V" => cmd_version(rest),
        "ask" => cmd_ask(rest),
        "mcp" => cmd_mcp(rest),
        "auth" => cmd_auth(rest),
        "doctor" => cmd_doctor(rest),
        _ => cmd_ask(&args),
    };
    ExitCode::from(code)
}

fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn cmd_version(args: &[String]) -> u8 {
    if has(args, "--json") {
        println!("{}", json!({"version": VERSION, "build": option_env!("RT_BUILD").unwrap_or("dev"), "engine": "rust"}));
    } else {
        println!("roundtable {VERSION}");
    }
    0
}

struct AskArgs {
    question: String,
    context: Option<String>,
    heads: Option<String>,
    rounds: u32,
    timeout: Option<u64>,
    research: bool,
    out: Option<String>,
    events: bool,
    no_history: bool,
    source: String,
}

fn parse_ask(args: &[String]) -> Result<AskArgs, String> {
    let mut a = AskArgs {
        question: String::new(),
        context: None,
        heads: None,
        rounds: 1,
        timeout: None,
        research: false,
        out: None,
        events: false,
        no_history: false,
        source: "terminal".into(),
    };
    let mut i = 0;
    let val = |i: &mut usize| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| format!("missing value for {}", args[*i - 1]))
    };
    while i < args.len() {
        match args[i].as_str() {
            "-q" | "--question" => a.question = val(&mut i)?,
            "-c" | "--context" => a.context = Some(val(&mut i)?),
            "--heads" => a.heads = Some(val(&mut i)?),
            "--rounds" => a.rounds = val(&mut i)?.parse().map_err(|_| "--rounds needs a number".to_string())?,
            "--timeout" => a.timeout = Some(val(&mut i)?.parse().map_err(|_| "--timeout needs seconds".to_string())?),
            "--research" => a.research = true,
            "--out" => a.out = Some(val(&mut i)?),
            "--events" => a.events = true,
            "--no-history" => a.no_history = true,
            "--source" => a.source = val(&mut i)?,
            other => {
                if !a.question.is_empty() {
                    a.question.push(' ');
                }
                a.question.push_str(other);
            }
        }
        i += 1;
    }
    Ok(a)
}

/// Keep a finished run in history unless turned off. Returns the id.
fn record(r: &RunResult, opts: &AskOptions, source: &str, settings: &Settings) -> Option<String> {
    if !settings.history {
        return None;
    }
    let e = history::Entry {
        id: history::new_id(),
        ts: history::now(),
        question: opts.question.clone(),
        source: source.into(),
        mode: if opts.rounds <= 1 { "advisory".into() } else { "deliberation".into() },
        heads: opts.heads.iter().map(|h| h.id.to_string()).collect(),
        consensus: r.consensus,
    };
    history::save(&paths::history_dir(), &e, &r.transcript).ok().map(|_| e.id)
}

fn cmd_ask(args: &[String]) -> u8 {
    let a = match parse_ask(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ERROR: {e}");
            return 2;
        }
    };
    if a.question.trim().is_empty() {
        eprintln!("ERROR: no question. Use -q \"...\".");
        return 2;
    }
    let settings = Settings::load();
    let store = keys::default_store();
    let k = Keys::load(store.as_ref());
    let heads = match &a.heads {
        Some(csv) => {
            let (h, warn) = council::parse_heads(csv);
            for w in warn {
                eprintln!("{w}");
            }
            h
        }
        None => council::default_heads(&settings, &k),
    };
    if heads.is_empty() {
        eprintln!("ERROR: no valid heads selected (after dropping unknown heads).");
        return 2;
    }
    let context = match &a.context {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("ERROR: could not read the context file: {e}");
                return 2;
            }
        },
        None => None,
    };
    let opts = AskOptions {
        question: a.question.clone(),
        context,
        heads,
        rounds: a.rounds.clamp(1, 10),
        research: a.research,
        timeout: a.timeout.map(std::time::Duration::from_secs).unwrap_or_else(|| council::default_timeout(a.research, &settings)),
    };
    let stdout = std::sync::Mutex::new(std::io::stdout());
    let emit = |e: &Event| {
        if a.events {
            let mut o = stdout.lock().unwrap();
            let _ = writeln!(o, "{}", serde_json::to_string(e).unwrap_or_default());
            let _ = o.flush();
        }
    };
    let r = council::run(&opts, &k, &settings, &emit);
    let id = if a.no_history { None } else { record(&r, &opts, &a.source, &settings) };
    if let Some(p) = &a.out {
        if let Err(e) = std::fs::write(p, &r.transcript) {
            eprintln!("WARN: could not save --out: {e}");
        }
    }
    if a.events {
        emit(&Event::RunDone { transcript: r.transcript.clone(), answered: r.answered, consensus: r.consensus, history_id: id });
    } else {
        print!("{}", r.transcript);
    }
    if r.answered > 0 {
        0
    } else {
        1
    }
}

fn cmd_mcp(args: &[String]) -> u8 {
    match args.first().map(String::as_str).unwrap_or("") {
        "serve" => {
            let settings = Settings::load();
            let store = keys::default_store();
            let run = |q: &str, heads: Option<&str>, rounds: Option<u32>, research: bool| -> (String, bool) {
                let k = Keys::load(store.as_ref());
                let hs = match heads {
                    Some(csv) => council::parse_heads(csv).0,
                    None => council::default_heads(&settings, &k),
                };
                if hs.is_empty() {
                    return ("No valid heads selected.".into(), true);
                }
                let opts = AskOptions {
                    question: q.into(),
                    context: None,
                    heads: hs,
                    rounds: rounds.unwrap_or(1),
                    research,
                    timeout: council::default_timeout(research, &settings),
                };
                let r = council::run(&opts, &k, &settings, &|_| {});
                record(&r, &opts, "claude", &settings);
                (r.transcript.trim().to_string(), false)
            };
            let stdin = std::io::stdin();
            match mcp::serve(stdin.lock(), std::io::stdout(), &run) {
                Ok(()) => 0,
                Err(_) => 0,
            }
        }
        "selftest" | "--selftest" => {
            let exe = match std::env::current_exe() {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("selftest: {e}");
                    return 1;
                }
            };
            let mut cmd = std::process::Command::new(exe);
            cmd.args(["mcp", "serve"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("selftest: {e}");
                    return 1;
                }
            };
            let _ = child.stdin.take().unwrap().write_all(mcp::selftest_input().as_bytes());
            let mut out = String::new();
            let _ = child.stdout.take().unwrap().read_to_string(&mut out);
            let _ = child.wait();
            match mcp::selftest_check(&out) {
                Ok(()) => {
                    println!("{}", json!({"ok": true}));
                    0
                }
                Err(e) => {
                    println!("{}", json!({"ok": false, "error": e}));
                    1
                }
            }
        }
        "config" => {
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "roundtable".into());
            println!("{}", serde_json::to_string_pretty(&json!({"mcpServers": {"roundtable": {"command": exe, "args": ["mcp", "serve"]}}})).unwrap());
            0
        }
        _ => {
            eprintln!("usage: roundtable mcp {{serve|selftest|config}}");
            2
        }
    }
}

fn auth_status_json(store: &dyn KeyStore) -> serde_json::Value {
    let k = Keys::load(store);
    let mut m = serde_json::Map::new();
    for h in providers::HEADS.iter().filter(|h| !h.env_keys.is_empty()) {
        let src = k.resolve(h).map(|(_, s)| s);
        m.insert(h.id.into(), json!({"provider": h.provider, "has_key": src.is_some(), "source": src}));
    }
    json!({"heads": m, "importable": keys::importable(&k.file).iter().map(|h| h.id).collect::<Vec<_>>()})
}

fn cmd_auth(args: &[String]) -> u8 {
    let store = keys::default_store();
    let json_out = has(args, "--json");
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let provider_arg = args.get(1).filter(|a| !a.starts_with("--")).map(String::as_str);
    let reply = |v: serde_json::Value, code: u8| -> u8 {
        if json_out {
            println!("{v}");
        } else if let Some(m) = v.get("message").and_then(|m| m.as_str()) {
            println!("{m}");
        } else {
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        code
    };
    match sub {
        "status" => reply(auth_status_json(store.as_ref()), 0),
        "set" => {
            let Some(h) = provider_arg.and_then(providers::find).filter(|h| !h.env_keys.is_empty()) else {
                return reply(json!({"ok": false, "message": "Unknown AI. Use one of: claude, openai, gemini, grok, glm, minimax."}), 2);
            };
            let mut raw = String::new();
            let _ = std::io::stdin().read_to_string(&mut raw);
            let key = keys::clean_key(&raw);
            if key.is_empty() {
                return reply(json!({"ok": false, "message": "No key was pasted."}), 2);
            }
            if has(args, "--check") {
                let c = verify::check_key(h, &key, &Settings::load());
                if !c.ok {
                    return reply(json!({"ok": false, "reason": c.reason, "message": c.message}), 3);
                }
            }
            match store.set(h.provider, &key) {
                Ok(()) => reply(json!({"ok": true, "head": h.id, "message": "Saved."}), 0),
                Err(e) => reply(json!({"ok": false, "reason": "keychain", "message": format!("Could not save the key: {e}")}), 4),
            }
        }
        "remove" => {
            let Some(h) = provider_arg.and_then(providers::find) else {
                return reply(json!({"ok": false, "message": "Unknown AI."}), 2);
            };
            match store.remove(h.provider) {
                Ok(()) => reply(json!({"ok": true, "head": h.id, "message": "Removed."}), 0),
                Err(e) => reply(json!({"ok": false, "message": e}), 4),
            }
        }
        "import" => {
            let k = Keys::load(store.as_ref());
            match keys::import_from_file(store.as_ref(), &k.file) {
                Ok(done) => {
                    let mut removed = 0;
                    if has(args, "--remove") && !done.is_empty() {
                        removed = keys::strip_keys_from_file(&paths::cli_config_file()).unwrap_or(0);
                    }
                    reply(json!({"ok": true, "imported": done, "removed_lines": removed}), 0)
                }
                Err(e) => reply(json!({"ok": false, "message": e}), 4),
            }
        }
        _ => {
            eprintln!("usage: roundtable auth {{set <provider>|remove <provider>|status|import [--remove]}} [--json]");
            2
        }
    }
}

fn cmd_doctor(args: &[String]) -> u8 {
    let store = keys::default_store();
    let k = Keys::load(store.as_ref());
    let settings = Settings::load();
    let online = has(args, "--online");
    let mut rows = Vec::new();
    for h in providers::HEADS {
        let resolved = k.resolve(h);
        let mut row = json!({
            "head": h.id,
            "name": h.display,
            "model": providers::model_for(h, &settings),
            "has_key": resolved.is_some(),
            "source": resolved.as_ref().map(|(_, s)| *s),
            "in_app": h.in_app,
        });
        if h.kind == providers::Kind::CodexCli {
            row["has_key"] = json!(council::find_exe("codex").is_some());
        }
        if online {
            if let Some((key, _)) = &resolved {
                let c = verify::check_key(h, key, &settings);
                row["reachable"] = json!(c.ok);
                row["error"] = json!(c.reason);
            }
        }
        rows.push(row);
    }
    let v = json!({"version": VERSION, "heads": rows, "data_dir": paths::data_dir(), "history": settings.history});
    if has(args, "--json") {
        println!("{v}");
    } else {
        for r in &rows {
            println!(
                "{:<8} {:<26} {}",
                r["head"].as_str().unwrap_or(""),
                r["model"].as_str().unwrap_or(""),
                if r["has_key"].as_bool() == Some(true) { "ready" } else { "no key" }
            );
        }
    }
    0
}
