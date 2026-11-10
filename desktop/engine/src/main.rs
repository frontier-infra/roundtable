//! `roundtable`: the engine command line. Same `ask` flags as the bash CLI,
//! plus machine-readable modes for the desktop app.

use roundtable_engine::council::{self, AskOptions, Event, RunResult};
use roundtable_engine::keys::{self, KeyStore, Keys};
use roundtable_engine::settings::Settings;
use roundtable_engine::verdict::Shape;
use roundtable_engine::{attach, clis, coach, history, mcp, paths, personas, providers, verify, VERSION};
use std::io::IsTerminal;
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
  detect         Find AI tools on this computer (Claude Code, Codex, Gemini CLI, ...)
                 and say if each is signed in. Sends no prompt. --json for the app.
  doctor         Show which AIs have a key (--online also checks each key)
  test <ai>      Ask one AI a five-word question and say how it went.
                 --via auto|app|key (default auto: its app first, then the key),
                 --timeout S (default 90), --json for the app, --verbose adds stderr.
  personas       List the persona packs, presets and seats (--json for the app)
  coach          questions -q Q | brief (JSON on stdin) --json: the question coach
  suggest        -q Q [--brief FILE] [--heads CSV] [--json]: suggest seats for a question
  attach         FILE|URL ... [--json]: read files and links once, as a run would share them
  mcp            serve | selftest | config
  version        Print the version

Ask flags:
  -q, --question TEXT   The question (also accepted as a positional)
  -c, --context FILE    Background text every AI reads first
      --heads CSV       Subset: claude,openai,grok,glm,minimax,gemini,codex,
                        qwen,kimi,opencode,cursor
      --rounds N        1 to 4. 1: each AI answers once (default). 2 to 4: they read
                        each other and revise toward consensus; Claude chairs
      --personas SPEC   Seat roles: a preset (pricing), a pack (business-roles),
                        or persona names (cfo,ceo@claude). generic: plain seats.
                        auto: suggest seats from the question (or the brief)
      --coach           The coach asks up to three clarifying questions first, then
                        writes a brief the table reads (goal, context, constraints,
                        what a good answer looks like)
      --coach-answer A  An answer to the coach's next question (repeat, in order)
      --coach-questions Print the coach's questions (Q: lines) and stop
      --brief FILE      A brief the table reads in front of the question
      --verdict SHAPE   The chair's final verdict: decision, pros-cons, plan or vote.
                        It always names the minority view and who held it
      --no-blind        Round 1 is open: each seat reads the seats before it
      --follow-up X     A follow-up: X is a past run's history id or a transcript file
      --attach SRC      A text file, PDF or link every seat reads (repeat). Long ones
                        are summarized once and the summary shared
      --attached FILE   Material already prepared by `roundtable attach`
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
        "detect" => cmd_detect(rest),
        "test" => cmd_test(rest),
        "personas" => cmd_personas(rest),
        "coach" => cmd_coach(rest),
        "suggest" => cmd_suggest(rest),
        "attach" => cmd_attach(rest),
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
    personas: Option<String>,
    rounds: u32,
    timeout: Option<u64>,
    research: bool,
    out: Option<String>,
    events: bool,
    no_history: bool,
    source: String,
    coach: bool,
    coach_answers: Vec<String>,
    coach_questions: bool,
    brief: Option<String>,
    verdict: Option<String>,
    open_first_round: bool,
    follow_up: Option<String>,
    attach: Vec<String>,
    attached: Option<String>,
}

fn parse_ask(args: &[String]) -> Result<AskArgs, String> {
    let mut a = AskArgs {
        question: String::new(),
        context: None,
        heads: None,
        personas: None,
        rounds: 1,
        timeout: None,
        research: false,
        out: None,
        events: false,
        no_history: false,
        source: "terminal".into(),
        coach: false,
        coach_answers: Vec::new(),
        coach_questions: false,
        brief: None,
        verdict: None,
        open_first_round: false,
        follow_up: None,
        attach: Vec::new(),
        attached: None,
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
            "--personas" => a.personas = Some(val(&mut i)?),
            "--rounds" => a.rounds = val(&mut i)?.parse().map_err(|_| "--rounds needs a number".to_string())?,
            "--timeout" => a.timeout = Some(val(&mut i)?.parse().map_err(|_| "--timeout needs seconds".to_string())?),
            "--research" => a.research = true,
            "--out" => a.out = Some(val(&mut i)?),
            "--events" => a.events = true,
            "--no-history" => a.no_history = true,
            "--source" => a.source = val(&mut i)?,
            "--coach" => a.coach = true,
            "--coach-answer" => {
                a.coach = true;
                a.coach_answers.push(val(&mut i)?)
            }
            "--coach-questions" => a.coach_questions = true,
            "--brief" => a.brief = Some(val(&mut i)?),
            "--verdict" => a.verdict = Some(val(&mut i)?),
            "--no-blind" | "--open-first-round" => a.open_first_round = true,
            "--blind" => a.open_first_round = false,
            "--follow-up" => a.follow_up = Some(val(&mut i)?),
            "--attach" => a.attach.push(val(&mut i)?),
            "--attached" => a.attached = Some(val(&mut i)?),
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
        heads: opts.table().iter().map(|s| s.head.id.to_string()).fold(Vec::new(), |mut v, h| {
            if !v.contains(&h) {
                v.push(h);
            }
            v
        }),
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
    let read = |what: &str, p: &str| std::fs::read_to_string(p).map_err(|e| format!("could not read the {what} file: {e}"));
    let context = match a.context.as_deref().map(|p| read("context", p)).transpose() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ERROR: {e}");
            return 2;
        }
    };
    if a.coach_questions {
        return match coach::ask_questions(&a.question, &k, &settings, coach::CHAIR_WAIT) {
            Ok(qs) if qs.is_empty() => {
                println!("NONE");
                0
            }
            Ok(qs) => {
                for q in qs {
                    println!("Q: {q}");
                }
                0
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
                1
            }
        };
    }
    let mut brief = match a.brief.as_deref().map(|p| read("brief", p)).transpose() {
        Ok(b) => b.map(|b| b.trim().to_string()).filter(|b| !b.is_empty()),
        Err(e) => {
            eprintln!("ERROR: {e}");
            return 2;
        }
    };
    if a.coach && brief.is_none() {
        brief = run_coach(&a.question, &a.coach_answers, &k, &settings, a.events);
    }
    let prior = match a.follow_up.as_deref() {
        Some(x) => match history::read(&paths::history_dir(), x).map(Ok).unwrap_or_else(|| read("follow-up", x)) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("ERROR: {e}");
                return 2;
            }
        },
        None => None,
    };
    let mut shared: Vec<String> = Vec::new();
    if let Some(p) = a.attached.as_deref() {
        match read("attached", p) {
            Ok(t) if !t.trim().is_empty() => shared.push(t.trim().to_string()),
            Ok(_) => {}
            Err(e) => {
                eprintln!("ERROR: {e}");
                return 2;
            }
        }
    }
    if !a.attach.is_empty() {
        match attach::prepare(&a.attach, &k, &settings, coach::CHAIR_WAIT) {
            Ok(list) => {
                for x in &list {
                    if !a.events {
                        eprintln!("NOTE: attached {} ({} characters, shared {}).", x.name, x.chars, x.shared);
                    }
                }
                shared.push(attach::block(&list));
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
                return 2;
            }
        }
    }
    let attachments = Some(shared.join("\n\n")).filter(|t| !t.trim().is_empty());
    let verdict = match Shape::parse(a.verdict.as_deref().unwrap_or("")) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ERROR: {e}");
            return 2;
        }
    };
    let mut spec = a.personas.clone().or_else(|| k.env.get("ROUNDTABLE_PERSONAS").cloned()).unwrap_or_default();
    let lib = personas::Library::load(&settings);
    if spec.trim().eq_ignore_ascii_case("auto") {
        let text = brief.clone().unwrap_or_else(|| a.question.clone());
        let sug = coach::suggest(&text, &lib, &k, &settings, coach::CHAIR_WAIT);
        if !a.events {
            eprintln!("NOTE: suggested seats: {}. {}", if sug.personas.is_empty() { "generic".to_string() } else { sug.personas.join(", ") }, sug.why);
        }
        spec = sug.personas.join(",");
    }
    let seats = match personas::resolve(&spec, &heads, &lib) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ERROR: {e}");
            return 2;
        }
    };
    if a.rounds > council::MAX_ROUNDS {
        eprintln!("NOTE: --rounds is at most {}. Running {}.", council::MAX_ROUNDS, council::MAX_ROUNDS);
    }
    let opts = AskOptions {
        question: a.question.clone(),
        context,
        prior,
        attachments,
        brief,
        open_first_round: a.open_first_round,
        verdict,
        heads,
        seats,
        rounds: a.rounds.clamp(1, council::MAX_ROUNDS),
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

/// `--coach`: the coach's questions, the person's answers (flags, else the terminal), then the brief.
/// None when the coach did not answer, or the person skipped the brief.
fn run_coach(question: &str, given: &[String], k: &Keys, settings: &Settings, quiet: bool) -> Option<String> {
    let tty = !quiet && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let qs = match coach::ask_questions(question, k, settings, coach::CHAIR_WAIT) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("NOTE: {e} Asking the question as typed.");
            return None;
        }
    };
    let mut qa = Vec::new();
    let stdin = std::io::stdin();
    for (i, q) in qs.iter().enumerate() {
        let a = match given.get(i) {
            Some(a) => a.clone(),
            None if tty => {
                eprint!("Coach: {q}\n> ");
                let mut line = String::new();
                let _ = stdin.read_line(&mut line);
                line.trim().to_string()
            }
            None => String::new(),
        };
        qa.push((q.clone(), a));
    }
    let brief = match coach::make_brief(question, &qa, k, settings, coach::CHAIR_WAIT) {
        Ok(b) => b.to_markdown(),
        Err(e) => {
            eprintln!("NOTE: {e} Asking the question as typed.");
            return None;
        }
    };
    if tty {
        eprintln!("\nThe brief the table will read:\n{brief}\n");
        eprint!("Press Enter to use it, or type s and Enter to skip it: ");
        let mut line = String::new();
        let _ = stdin.read_line(&mut line);
        if line.trim().eq_ignore_ascii_case("s") {
            return None;
        }
    }
    Some(brief)
}

/// `roundtable coach questions -q Q --json` and `roundtable coach brief --json` (JSON on stdin:
/// `{"question": "...", "qa": [{"q": "...", "a": "..."}]}`).
fn cmd_coach(args: &[String]) -> u8 {
    let settings = Settings::load();
    let store = keys::default_store();
    let k = Keys::load(store.as_ref());
    let json_out = has(args, "--json");
    let reply = |v: serde_json::Value, text: String, code: u8| -> u8 {
        if json_out {
            println!("{v}");
        } else {
            println!("{text}");
        }
        code
    };
    match args.first().map(String::as_str).unwrap_or("") {
        "questions" => {
            let q = flag_value(args, "-q").or_else(|| flag_value(args, "--question")).unwrap_or("").to_string();
            if q.trim().is_empty() {
                eprintln!("usage: roundtable coach questions -q \"question\" [--json]");
                return 2;
            }
            match coach::ask_questions(&q, &k, &settings, coach::CHAIR_WAIT) {
                Ok(qs) => {
                    let text = if qs.is_empty() { "NONE".to_string() } else { qs.iter().map(|q| format!("Q: {q}")).collect::<Vec<_>>().join("\n") };
                    reply(json!({"ok": true, "questions": qs}), text, 0)
                }
                Err(e) => reply(json!({"ok": false, "questions": [], "message": e}), e.clone(), 1),
            }
        }
        "brief" => {
            let mut raw = String::new();
            let _ = std::io::stdin().read_to_string(&mut raw);
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or(json!({}));
            let q = v["question"].as_str().unwrap_or("").to_string();
            if q.trim().is_empty() {
                return reply(json!({"ok": false, "message": "No question was given."}), "No question was given.".into(), 2);
            }
            let qa: Vec<(String, String)> = v["qa"]
                .as_array()
                .map(|a| a.iter().map(|x| (x["q"].as_str().unwrap_or("").to_string(), x["a"].as_str().unwrap_or("").to_string())).collect())
                .unwrap_or_default();
            match coach::make_brief(&q, &qa, &k, &settings, coach::CHAIR_WAIT) {
                Ok(b) => reply(json!({"ok": true, "brief": b, "markdown": b.to_markdown()}), b.to_markdown(), 0),
                Err(e) => {
                    let b = coach::Brief { goal: q.trim().to_string(), ..Default::default() };
                    reply(json!({"ok": false, "message": e, "brief": b, "markdown": b.to_markdown()}), e.clone(), 1)
                }
            }
        }
        _ => {
            eprintln!("usage: roundtable coach {{questions -q Q|brief}} [--json]");
            2
        }
    }
}

/// `roundtable suggest -q Q [--brief FILE] [--heads CSV] [--json]`: seats for a question.
fn cmd_suggest(args: &[String]) -> u8 {
    let settings = Settings::load();
    let store = keys::default_store();
    let k = Keys::load(store.as_ref());
    let mut text = flag_value(args, "-q").or_else(|| flag_value(args, "--question")).unwrap_or("").to_string();
    if let Some(p) = flag_value(args, "--brief") {
        match std::fs::read_to_string(p) {
            Ok(b) if !b.trim().is_empty() => text = format!("{}\n\n{}", text.trim(), b.trim()),
            _ => {}
        }
    }
    if text.trim().is_empty() {
        eprintln!("usage: roundtable suggest -q \"question\" [--brief FILE] [--heads CSV] [--json]");
        return 2;
    }
    let heads = match flag_value(args, "--heads") {
        Some(csv) => council::parse_heads(csv).0,
        None => council::default_heads(&settings, &k),
    };
    let lib = personas::Library::load(&settings);
    let sug = coach::suggest(&text, &lib, &k, &settings, coach::CHAIR_WAIT);
    let seats = personas::resolve(&sug.personas.join(","), &heads, &lib).unwrap_or_default();
    let keys_of = personas::seat_keys(&seats);
    let rows: Vec<_> = seats
        .iter()
        .zip(&keys_of)
        .map(|(s, key)| json!({"seat": key, "head": s.head.id, "persona": s.persona.as_ref().map(|p| p.id.clone()), "name": s.persona.as_ref().map(|p| p.name.clone())}))
        .collect();
    let spec = if sug.personas.is_empty() { String::new() } else { personas::seats_spec(&seats) };
    if has(args, "--json") {
        println!("{}", json!({"source": sug.source, "personas": sug.personas, "why": sug.why, "seats": rows, "spec": spec}));
    } else if sug.personas.is_empty() {
        println!("Generic seats. {}", sug.why);
    } else {
        println!("--personas {spec}");
        println!("{}", sug.why);
    }
    0
}

/// `roundtable attach SRC... [--json]`: read files and links once and share them as a run would.
fn cmd_attach(args: &[String]) -> u8 {
    let srcs: Vec<String> = args.iter().filter(|a| !a.starts_with("--")).cloned().collect();
    if srcs.is_empty() {
        eprintln!("usage: roundtable attach FILE|URL ... [--json]");
        return 2;
    }
    let settings = Settings::load();
    let store = keys::default_store();
    let k = Keys::load(store.as_ref());
    match attach::prepare(&srcs, &k, &settings, coach::CHAIR_WAIT) {
        Ok(list) => {
            let block = attach::block(&list);
            if has(args, "--json") {
                println!("{}", json!({"ok": true, "attachments": list, "block": block}));
            } else {
                println!("{block}");
            }
            0
        }
        Err(e) => {
            if has(args, "--json") {
                println!("{}", json!({"ok": false, "message": e}));
            } else {
                eprintln!("ERROR: {e}");
            }
            1
        }
    }
}

fn cmd_mcp(args: &[String]) -> u8 {
    match args.first().map(String::as_str).unwrap_or("") {
        "serve" => {
            let settings = Settings::load();
            let store = keys::default_store();
            let run = |c: &mcp::Call| -> (String, bool) {
                let k = Keys::load(store.as_ref());
                let hs = match c.heads.as_deref() {
                    Some(csv) => council::parse_heads(csv).0,
                    None => council::default_heads(&settings, &k),
                };
                if hs.is_empty() {
                    return ("No valid heads selected.".into(), true);
                }
                let verdict = match Shape::parse(c.verdict.as_deref().unwrap_or("")) {
                    Ok(v) => v,
                    Err(e) => return (e, true),
                };
                // The coach: questions first, unless the caller brings the answers.
                let mut brief = None;
                if c.coach {
                    match coach::ask_questions(&c.question, &k, &settings, coach::CHAIR_WAIT) {
                        Ok(qs) if !qs.is_empty() && c.coach_answers.is_empty() => return (mcp::coach_reply(&qs), false),
                        Ok(qs) => {
                            let qa: Vec<(String, String)> = qs.iter().cloned().zip(c.coach_answers.iter().cloned().chain(std::iter::repeat(String::new()))).collect();
                            brief = coach::make_brief(&c.question, &qa, &k, &settings, coach::CHAIR_WAIT).ok().map(|b| b.to_markdown());
                        }
                        Err(_) => {}
                    }
                }
                let attachments = if c.attach.is_empty() {
                    None
                } else {
                    match attach::prepare(&c.attach, &k, &settings, coach::CHAIR_WAIT) {
                        Ok(list) => Some(attach::block(&list)),
                        Err(e) => return (e, true),
                    }
                };
                let lib = personas::Library::load(&settings);
                let mut spec = c.personas.clone().unwrap_or_default();
                if spec.trim().eq_ignore_ascii_case("auto") {
                    spec = coach::suggest(brief.as_deref().unwrap_or(&c.question), &lib, &k, &settings, coach::CHAIR_WAIT).personas.join(",");
                }
                let seats = match personas::resolve(&spec, &hs, &lib) {
                    Ok(s) => s,
                    Err(e) => return (e, true),
                };
                let opts = AskOptions {
                    question: c.question.clone(),
                    heads: hs,
                    seats,
                    rounds: c.rounds.unwrap_or(1).clamp(1, council::MAX_ROUNDS),
                    research: c.research,
                    timeout: council::default_timeout(c.research, &settings),
                    brief,
                    attachments,
                    verdict,
                    open_first_round: c.blind == Some(false),
                    ..Default::default()
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
        if let Some(c) = clis::for_head(h.id) {
            let found = clis::find_cli(c).is_some();
            row["cli"] = json!(found);
            row["via_cli"] = json!(providers::uses_cli(h, &settings));
            if h.kind == providers::Kind::Cli {
                row["has_key"] = json!(found);
            }
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

fn cmd_detect(args: &[String]) -> u8 {
    let found = clis::detect_all();
    if has(args, "--json") {
        println!("{}", json!({"clis": found}));
    } else {
        for d in &found {
            let state = match (d.found, d.signed_in) {
                (false, _) => "not found".to_string(),
                (true, Some(true)) => "signed in".to_string(),
                (true, Some(false)) => format!("not signed in (run: {})", d.login),
                (true, None) => "found, sign-in unknown".to_string(),
            };
            println!("{:<9} {:<20} {}", d.id, d.version.as_deref().unwrap_or("-"), state);
        }
    }
    0
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// `roundtable test <ai>`: one five-word question through the route a real run uses.
fn cmd_test(args: &[String]) -> u8 {
    let json_out = has(args, "--json");
    let Some(head) = args.first().filter(|a| !a.starts_with("--")).and_then(|a| providers::find(a)) else {
        eprintln!("usage: roundtable test <ai> [--via auto|app|key] [--timeout S] [--json]");
        return 2;
    };
    let route = match flag_value(args, "--via").unwrap_or("auto") {
        "app" | "tool" | "cli" => council::Route::Tool,
        "key" => council::Route::Key,
        _ => council::Route::Auto,
    };
    let secs = flag_value(args, "--timeout").and_then(|v| v.parse().ok()).unwrap_or(90);
    let settings = Settings::load();
    let store = keys::default_store();
    let k = Keys::load(store.as_ref());
    let t0 = std::time::Instant::now();
    let r = council::ask_head(head, clis::TEST_PROMPT, false, std::time::Duration::from_secs(secs), &k, &settings, route, None);
    let elapsed = (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0;
    let text = match &r.outcome {
        council::Outcome::Answered(t) => Some(t.clone()),
        _ => None,
    };
    let first = text.as_deref().and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty())).map(str::to_string);
    let mut v = json!({
        "head": head.id,
        "name": head.display,
        "ok": text.is_some(),
        "status": r.outcome.status(),
        "via": r.via,
        "first_line": first,
        "message": r.outcome.message(),
        "fix": r.outcome.fix(),
        "secs": elapsed,
        "command": r.trial.as_ref().map(|t| t.command.clone()),
        "exit": r.trial.as_ref().and_then(|t| t.exit),
    });
    if has(args, "--verbose") {
        v["text"] = json!(text);
        v["stderr_tail"] = json!(r.trial.as_ref().map(|t| t.stderr_tail.clone()));
    }
    if json_out {
        println!("{v}");
    } else {
        let via = r.via.as_deref().unwrap_or("nothing");
        match &first {
            Some(l) => println!("{}: answered in {elapsed} s via {via}: {l}", head.display),
            None => println!("{}: {} ({via})", head.display, r.outcome.message().unwrap_or_default()),
        }
        if let Some(t) = &r.trial {
            println!("  ran: {}  exit {}  {} s", t.command, t.exit.map(|c| c.to_string()).unwrap_or_else(|| "timeout".into()), t.secs);
        }
    }
    if text.is_some() {
        0
    } else {
        1
    }
}

fn cmd_personas(args: &[String]) -> u8 {
    let settings = Settings::load();
    if has(args, "--json") {
        println!("{}", personas::library_json(&settings));
        return 0;
    }
    let lib = personas::Library::load(&settings);
    println!("Generic: plain seats, one per AI (--personas generic).");
    for p in &lib.packs {
        println!();
        println!("{} ({})", p.name, p.id);
        for x in &p.personas {
            let ai = x.ai.as_deref().map(|a| format!(", prefers {a}")).unwrap_or_default();
            println!("  {:<22} {}{ai}", x.id, x.name);
        }
        for pr in &p.presets {
            println!("  preset {:<15} {}", pr.id, pr.seats.join(", "));
        }
    }
    println!();
    if personas::labs_hidden(&settings) {
        println!("The Frontier labs pack is hidden.");
    } else {
        println!("Lab seats are simulated perspectives drawn from public positions, not affiliated with or endorsed by any lab.");
    }
    0
}
