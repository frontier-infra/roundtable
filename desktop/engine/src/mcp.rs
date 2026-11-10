//! MCP server: the single `roundtable` tool, with the same schema as `lib/mcp.py`
//! plus the Rust-only arguments (`seats`, `files`, `full_transcript`).
//!
//! One `Server` answers JSON-RPC 2.0 messages for both transports:
//! - stdio (`roundtable mcp serve`): newline-delimited JSON, stdout carries only MCP messages.
//! - Streamable HTTP (`roundtable mcp serve --http`): see `mcp_http`.
//!
//! A long run sends `notifications/progress` when the caller passed a progress token,
//! with a heartbeat between seats so clients that reset their timeout on progress
//! keep waiting. The result leads with the verdict; the full transcript comes as an
//! embedded resource (`roundtable://run/<id>`), also readable with `resources/read`.

use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// How many finished runs a server keeps for `resources/read`.
const KEEP_RUNS: usize = 20;
/// Inline files a remote caller may send: count, and characters in all.
pub const MAX_FILES: usize = 10;
pub const MAX_FILE_CHARS: usize = 2_000_000;

/// The tool. `remote` is the HTTP transport: `attach` takes links only there.
pub fn tool(remote: bool) -> Value {
    let attach = if remote {
        "Optional. http(s) links that every seat reads (public addresses only; this connection cannot read files on the computer). To share a document, put its text in files."
    } else {
        "Optional. Paths of text or PDF files, or http(s) links, that every seat reads. Each is read once; a long one is summarized once and the summary shared."
    };
    json!({
        "name": "roundtable",
        "title": "Roundtable",
        "description": "Convene a council of frontier AI models (Grok, Codex/OpenAI, GLM, MiniMax, Claude, Gemini) on a question. Returns the chair's verdict first, then the full markdown transcript as a resource. With rounds>1 the council deliberates across rounds until a Claude chair declares consensus. Use this when a decision, plan or review would benefit from several independent AI opinions. A run takes one to several minutes. Heads with no configured key/CLI are skipped gracefully. Roundtable is free from Titanium Computing (https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=mcp).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "The question to put to the council."},
                "heads": {"type": "string", "description": "Optional comma-separated subset of heads to convene (any of: grok,codex,openai,glm,minimax,claude,gemini). Defaults to all configured heads."},
                "seats": {"description": "Optional. Seats from providers added in the Roundtable app or with `roundtable provider add` (OpenRouter, Ollama, LM Studio, Groq, Together, any OpenAI-compatible URL), as provider:model, for example ollama:llama3.2 or openrouter:openai/gpt-4o-mini. A comma-separated string or an array. Alone, the table is exactly these seats; with heads, they are added.", "anyOf": [{"type": "string"}, {"type": "array", "items": {"type": "string"}}]},
                "rounds": {"type": "integer", "description": "Optional number of rounds, 1 to 4. 1 (default) = advisory (blind, independent answers); 2 to 4 = the seats read each other and revise toward consensus, and a Claude chair gives the verdict, stopping early when they agree.", "minimum": 1, "maximum": 4},
                "mode": {"type": "string", "enum": ["quick", "deep"], "description": "Optional. quick (the default): one round, short seat answers and a short verdict (the pick, confidence, what would change it, three next steps, one minority view), about 2 minutes. deep: Deep Think, for complex asks (several stakeholders, money or legal stakes, more than three options, missing facts): the coach first, up to 4 rounds that argue against the leading pick, long seat answers and a long verdict (reasoning by theme, trade-offs, risks, unknowns and how to resolve them, a step plan, the minority view in full). It takes about 14 minutes. rounds, when given, overrides the mode's rounds."},
                "personas": {"type": "string", "description": "Optional seat roles. A preset (security-review, go-to-market, hiring-decision, pricing, architecture-review, exec-team, board-review, frontier-four), a pack (business-roles, frontier-labs), or a comma list of persona ids or names, each optionally ending in @<head> (cfo,ceo@claude). 'generic' or empty = plain seats. Lab seats are simulated perspectives drawn from public positions, not affiliated with or endorsed by the lab."},
                "research": {"type": "boolean", "description": "Optional. Enable web research / deeper reasoning for heads that support it (longer per-head timeout). Defaults to false."},
                "coach": {"type": "boolean", "description": "Optional. The question coach: one AI (the chair's seat) asks at most three clarifying questions, then writes a brief (goal, context, constraints, what a good answer looks like) that the table reads. Without coach_answers, the tool returns the questions; ask the person, then call again with the same question, coach true, and coach_answers."},
                "coach_answers": {"type": "array", "items": {"type": "string"}, "description": "Optional. The answers to the coach's questions, in order."},
                "verdict": {"type": "string", "enum": ["decision", "pros-cons", "plan", "vote", "none"], "description": "Optional. The chair's final verdict shape: a decision (the default with two or more seats), pros and cons, an action plan, a vote, or none. Every verdict leads with the pick, its confidence, what would change it and three next steps, and names the minority view and who held it. It opens with QUESTIONS FIRST when the question lacks key facts."},
                "blind": {"type": "boolean", "description": "Optional. Round 1 is blind (true, the default): seats answer without seeing each other. false lets each seat in round 1 read the seats before it."},
                "attach": {"type": "array", "items": {"type": "string"}, "description": attach},
                "decision": {"type": "string", "description": "Optional. After the final round, a decision model rates the options the table argued (plus 'need more information') with calibrated probabilities: jev (TypeSafe, needs a key), jebadiah (local Jebadiah), systemone (another System One model set in Settings, for example LINK21), or off. Defaults to the saved setting (off unless chosen).", "enum": ["jev", "jebadiah", "systemone", "link21", "off"]},
                "files": {"type": "array", "description": "Optional. Documents the person shared with you, as text, that every seat reads: [{\"name\": \"notes.md\", \"text\": \"...\"}]. At most 10, 2 million characters in all.", "items": {"type": "object", "properties": {"name": {"type": "string"}, "text": {"type": "string"}}, "required": ["name", "text"]}},
                "full_transcript": {"type": "boolean", "description": "Optional. Also put the full transcript in the text reply after the verdict (for clients that do not read embedded resources). Defaults to false."}
            },
            "required": ["question"],
            "additionalProperties": false
        },
        "annotations": {"title": "Roundtable", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true}
    })
}

/// One tool call's arguments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Call {
    pub question: String,
    pub heads: Option<String>,
    /// Provider seats (`provider:model`), comma-separated. Rust engine only.
    pub seats: Option<String>,
    pub rounds: Option<u32>,
    /// quick or deep. None: quick.
    pub mode: Option<String>,
    pub research: bool,
    pub personas: Option<String>,
    pub coach: bool,
    pub coach_answers: Vec<String>,
    pub verdict: Option<String>,
    /// None: the default (blind).
    pub blind: Option<bool>,
    pub attach: Vec<String>,
    /// Decision model for this call: jev, jebadiah, systemone, link21 or off. None: the saved setting.
    pub decision: Option<String>,
    /// Inline documents: (name, text).
    pub files: Vec<(String, String)>,
    pub full_transcript: bool,
    /// The call came over HTTP: links only, never local paths.
    pub remote: bool,
    /// Who asked, for history: "claude", "mcp" or "remote".
    pub source: String,
}

/// What one tool call produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// A plain reply: an error, or the coach's questions.
    Message { text: String, is_error: bool },
    /// A finished roundtable.
    Run {
        /// The chair's final verdict, when there was one.
        verdict: Option<String>,
        transcript: String,
        answered: usize,
        seats: usize,
        rounds: u32,
        consensus: Option<bool>,
        /// The history id, when the run was kept.
        history_id: Option<String>,
    },
}

impl Outcome {
    pub fn error(text: impl Into<String>) -> Self {
        Outcome::Message { text: text.into(), is_error: true }
    }
    pub fn text(text: impl Into<String>) -> Self {
        Outcome::Message { text: text.into(), is_error: false }
    }
}

/// Progress from a run: (steps done, steps in all, one plain line).
pub type Progress<'a> = dyn Fn(f64, Option<f64>, &str) + Sync + 'a;
/// One tool call. It may report progress as it goes.
pub type Runner<'a> = dyn Fn(&Call, &Progress) -> Outcome + Sync + 'a;

/// What the tool returns when the coach has questions and no answers came with the call.
pub fn coach_reply(questions: &[String]) -> String {
    let mut s = String::from("The coach has questions before the table meets. Ask the person (or answer from what you already know), then call roundtable again with the same question, coach: true, and coach_answers in this order:\n");
    for (i, q) in questions.iter().enumerate() {
        s.push_str(&format!("\n{}. {q}", i + 1));
    }
    s
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        _ => Vec::new(),
    }
}

fn files(v: Option<&Value>) -> Result<Vec<(String, String)>, String> {
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let Some(list) = v.as_array() else { return Err("'files' must be an array of {name, text}".into()) };
    if list.len() > MAX_FILES {
        return Err(format!("'files' takes at most {MAX_FILES} documents"));
    }
    let mut out = Vec::new();
    let mut total = 0;
    for (i, f) in list.iter().enumerate() {
        let text = f.get("text").or_else(|| f.get("content")).and_then(|t| t.as_str()).ok_or_else(|| format!("files[{i}] needs a text"))?;
        let name = f.get("name").and_then(|t| t.as_str()).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| format!("file {}", i + 1));
        total += text.chars().count();
        if total > MAX_FILE_CHARS {
            return Err(format!("'files' is larger than {MAX_FILE_CHARS} characters in all"));
        }
        if !text.trim().is_empty() {
            out.push((name, text.to_string()));
        }
    }
    Ok(out)
}

/// True for an http(s) link (the only `attach` a remote caller may send).
pub fn is_link(s: &str) -> bool {
    let l = s.trim().to_ascii_lowercase();
    l.starts_with("https://") || l.starts_with("http://")
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

pub fn err(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Parse a `tools/call`'s arguments. Err is a -32602 message.
pub fn parse_call(args: &Value, remote: bool) -> Result<Call, String> {
    let q = args.get("question").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if q.is_empty() {
        return Err("Missing required argument: 'question' (non-empty string)".into());
    }
    let rounds = match args.get("rounds") {
        None | Some(Value::Null) => None,
        Some(v) => match v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())) {
            Some(n) => Some(n.clamp(1, 4) as u32),
            None => return Err("'rounds' must be an integer".into()),
        },
    };
    let text_arg = |k: &str| args.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let attach = strings(args.get("attach"));
    if remote {
        if let Some(bad) = attach.iter().find(|a| !is_link(a)) {
            let _ = bad;
            return Err("On a remote connection, 'attach' takes http(s) links only. Put a document's text in 'files'.".into());
        }
    }
    Ok(Call {
        question: q,
        heads: text_arg("heads"),
        seats: Some(strings(args.get("seats")).join(",")).filter(|s| !s.is_empty()),
        rounds,
        mode: text_arg("mode"),
        research: args.get("research").and_then(|v| v.as_bool()).unwrap_or(false),
        personas: text_arg("personas"),
        coach: args.get("coach").and_then(|v| v.as_bool()).unwrap_or(false),
        coach_answers: strings(args.get("coach_answers")),
        verdict: text_arg("verdict"),
        blind: args.get("blind").and_then(|v| v.as_bool()),
        attach,
        decision: text_arg("decision"),
        files: files(args.get("files"))?,
        full_transcript: args.get("full_transcript").and_then(|v| v.as_bool()).unwrap_or(false),
        remote,
        source: String::new(),
    })
}

/// A finished run kept for `resources/read`.
#[derive(Debug, Clone)]
struct Kept {
    id: String,
    question: String,
    transcript: String,
}

pub fn run_uri(id: &str) -> String {
    format!("roundtable://run/{id}")
}

/// Progress for one call: always increasing, as the spec asks.
struct Tracker<'a> {
    token: Value,
    notify: &'a (dyn Fn(Value) + Sync),
    /// Last progress sent (-1 before the first), steps done, steps in all, the last message.
    state: Mutex<(f64, f64, Option<f64>, String)>,
    started: Instant,
}

impl Tracker<'_> {
    fn send(&self, st: &mut (f64, f64, Option<f64>, String), message: &str) {
        let (last, done) = (st.0, st.1);
        // A new step jumps to its count; a heartbeat creeps toward the next step and never reaches it.
        let p = if done > last { done } else { last + (done + 1.0 - last) * 0.1 };
        st.0 = p;
        let mut params = json!({"progressToken": self.token, "progress": (p * 1000.0).round() / 1000.0, "message": message});
        if let Some(t) = st.2 {
            params["total"] = json!(t.max(p.ceil()));
        }
        (self.notify)(json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": params}));
    }

    fn report(&self, done: f64, total: Option<f64>, message: &str) {
        let mut st = self.state.lock().unwrap();
        st.1 = st.1.max(done);
        if total.is_some() {
            st.2 = total;
        }
        st.3 = message.to_string();
        self.send(&mut st, message);
    }

    fn beat(&self) {
        let mut st = self.state.lock().unwrap();
        let secs = self.started.elapsed().as_secs();
        let still = format!("Still working ({}m {:02}s).", secs / 60, secs % 60);
        let line = if st.3.is_empty() { still } else { format!("{} {still}", st.3) };
        self.send(&mut st, &line);
    }
}

/// One MCP server. Both transports share it.
pub struct Server<'a> {
    run: &'a Runner<'a>,
    remote: bool,
    /// Seconds between heartbeats while a run is going.
    pub heartbeat: Duration,
    runs: Mutex<Vec<Kept>>,
    client: Mutex<String>,
    seq: std::sync::atomic::AtomicU64,
}

impl<'a> Server<'a> {
    pub fn new(run: &'a Runner<'a>, remote: bool) -> Self {
        Server { run, remote, heartbeat: Duration::from_secs(10), runs: Mutex::new(Vec::new()), client: Mutex::new(String::new()), seq: Default::default() }
    }

    pub fn remote(&self) -> bool {
        self.remote
    }

    /// Who is calling, from `initialize`'s clientInfo.
    pub fn client(&self) -> String {
        self.client.lock().unwrap().clone()
    }

    fn source(&self) -> String {
        if self.remote {
            "remote".into()
        } else if self.client().to_ascii_lowercase().contains("claude") || self.client().is_empty() {
            "claude".into()
        } else {
            "mcp".into()
        }
    }

    fn keep(&self, id: &str, question: &str, transcript: &str) {
        let mut runs = self.runs.lock().unwrap();
        runs.retain(|r| r.id != id);
        runs.push(Kept { id: id.into(), question: question.into(), transcript: transcript.into() });
        let n = runs.len();
        if n > KEEP_RUNS {
            runs.drain(..n - KEEP_RUNS);
        }
    }

    /// The tool result for one outcome.
    pub fn result(&self, call: &Call, o: Outcome) -> Value {
        match o {
            Outcome::Message { text, is_error } => json!({"content": [{"type": "text", "text": text}], "isError": is_error}),
            Outcome::Run { verdict, transcript, answered, seats, rounds, consensus, history_id } => {
                let id = history_id.unwrap_or_else(|| {
                    let n = self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    format!("{}-mcp{n}", crate::history::now())
                });
                let transcript = transcript.trim().to_string();
                self.keep(&id, &call.question, &transcript);
                let uri = run_uri(&id);
                let mut head = format!(
                    "Roundtable: {answered} of {seats} seat{} answered, {rounds} round{}",
                    if seats == 1 { "" } else { "s" },
                    if rounds == 1 { "" } else { "s" }
                );
                match consensus {
                    Some(true) => head.push_str(", consensus reached."),
                    Some(false) => head.push_str(", no consensus."),
                    None => head.push('.'),
                }
                let mut content = Vec::new();
                match verdict.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                    Some(v) => {
                        let mut text = format!("{head}\n\n## Verdict\n\n{v}\n\n");
                        if call.full_transcript {
                            text.push_str(&format!("## Full transcript\n\n{transcript}\n"));
                        } else {
                            text.push_str(&format!("Every seat's full answer is in the transcript, attached as {uri}."));
                        }
                        content.push(json!({"type": "text", "text": text}));
                        if !call.full_transcript {
                            content.push(json!({"type": "resource", "resource": {"uri": uri, "mimeType": "text/markdown", "text": transcript}}));
                        }
                    }
                    None => {
                        // No verdict (one seat, or verdict: none): the answers are the reply.
                        content.push(json!({"type": "text", "text": format!("{head} No verdict for this table; the answers follow.\n\n{transcript}\n\n(Transcript: {uri})")}));
                    }
                }
                json!({"content": content, "isError": answered == 0})
            }
        }
    }

    fn call_tool(&self, id: &Value, params: &Value, notify: &(dyn Fn(Value) + Sync)) -> Value {
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name != "roundtable" {
            return err(id, -32602, &format!("Unknown tool: '{name}'"));
        }
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        let mut call = match parse_call(&args, self.remote) {
            Ok(c) => c,
            Err(e) => return err(id, -32602, &e),
        };
        call.source = self.source();
        let token = params.pointer("/_meta/progressToken").cloned().filter(|t| t.is_string() || t.is_number());
        let tracker = token.map(|token| Tracker { token, notify, state: Mutex::new((-1.0, 0.0, None, String::new())), started: Instant::now() });
        let report = |done: f64, total: Option<f64>, msg: &str| {
            if let Some(t) = &tracker {
                t.report(done, total, msg);
            }
        };
        let stop = (Mutex::new(false), Condvar::new());
        let outcome = std::thread::scope(|s| {
            if let Some(t) = &tracker {
                let (stop, every) = (&stop, self.heartbeat);
                s.spawn(move || {
                    let mut done = stop.0.lock().unwrap();
                    loop {
                        let (g, w) = stop.1.wait_timeout(done, every).unwrap();
                        done = g;
                        if *done {
                            break;
                        }
                        if w.timed_out() {
                            t.beat();
                        }
                    }
                });
            }
            let o = (self.run)(&call, &report);
            *stop.0.lock().unwrap() = true;
            stop.1.notify_all();
            o
        });
        ok(id, self.result(&call, outcome))
    }

    /// Route one message. Notifications get `None`. `notify` sends progress to the caller.
    pub fn dispatch(&self, msg: &Value, notify: &(dyn Fn(Value) + Sync)) -> Option<Value> {
        let Some(obj) = msg.as_object() else { return Some(err(&Value::Null, -32600, "Invalid JSON-RPC 2.0 request")) };
        let id = obj.get("id").cloned();
        if obj.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
            return id.map(|i| err(&i, -32600, "Invalid JSON-RPC 2.0 request"));
        }
        let Some(method) = obj.get("method").and_then(|m| m.as_str()) else {
            return id.map(|i| err(&i, -32600, "Missing or invalid 'method'"));
        };
        let id = id?;
        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        Some(match method {
            "initialize" => {
                let req = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("");
                let v = if KNOWN.contains(&req) { req } else { DEFAULT_PROTOCOL_VERSION };
                if let Some(n) = params.pointer("/clientInfo/name").and_then(|v| v.as_str()) {
                    *self.client.lock().unwrap() = n.chars().take(80).collect();
                }
                ok(&id, json!({
                    "protocolVersion": v,
                    "capabilities": {"tools": {"listChanged": false}, "resources": {"listChanged": false}},
                    "serverInfo": {"name": "roundtable", "title": "Roundtable", "version": crate::VERSION},
                    "instructions": "Call the 'roundtable' tool with a question to convene a council of frontier AI models. It returns the chair's verdict first and the full transcript as a resource. Use rounds>1 for chaired deliberation to consensus. A run takes one to several minutes; progress notifications are sent while it runs."
                }))
            }
            "tools/list" => ok(&id, json!({"tools": [tool(self.remote)]})),
            "ping" => ok(&id, json!({})),
            "tools/call" => self.call_tool(&id, &params, notify),
            "resources/list" => {
                let runs = self.runs.lock().unwrap();
                let list: Vec<Value> = runs
                    .iter()
                    .rev()
                    .map(|r| json!({"uri": run_uri(&r.id), "name": format!("Roundtable: {}", r.question.chars().take(80).collect::<String>()), "mimeType": "text/markdown", "description": "The full transcript of a roundtable run in this session."}))
                    .collect();
                ok(&id, json!({"resources": list}))
            }
            "resources/templates/list" => ok(&id, json!({"resourceTemplates": []})),
            "resources/read" => {
                let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                let runs = self.runs.lock().unwrap();
                match runs.iter().find(|r| run_uri(&r.id) == uri) {
                    Some(r) => ok(&id, json!({"contents": [{"uri": uri, "mimeType": "text/markdown", "text": r.transcript}]})),
                    None => err(&id, -32002, &format!("Resource not found: '{uri}'")),
                }
            }
            other => err(&id, -32601, &format!("Method not found: {other}")),
        })
    }
}

/// The stdio transport. Progress notifications share stdout with replies, one line each.
pub fn serve<R: BufRead, W: Write + Send>(input: R, output: W, server: &Server) -> std::io::Result<()> {
    let out = Mutex::new(output);
    let write = |v: &Value| -> std::io::Result<()> {
        let mut o = out.lock().unwrap();
        writeln!(o, "{}", serde_json::to_string(v)?)?;
        o.flush()
    };
    let notify = |v: Value| {
        let _ = write(&v);
    };
    for line in input.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => server.dispatch(&msg, &notify),
            Err(_) => Some(err(&Value::Null, -32700, "Parse error")),
        };
        if let Some(r) = reply {
            write(&r)?;
        }
    }
    Ok(())
}

/// The selftest handshake: initialize, initialized, tools/list. No network.
pub fn selftest_input() -> String {
    [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":DEFAULT_PROTOCOL_VERSION,"capabilities":{},"clientInfo":{"name":"selftest","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    ]
    .iter()
    .map(|v| v.to_string() + "\n")
    .collect()
}

/// Check the replies to `selftest_input`.
pub fn selftest_check(stdout: &str) -> Result<(), String> {
    let lines: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|_| format!("non-JSON line on stdout: {l}")))
        .collect::<Result<_, _>>()?;
    if lines.len() != 2 {
        return Err(format!("expected 2 replies, got {}", lines.len()));
    }
    if lines[0].pointer("/result/serverInfo/name") != Some(&json!("roundtable")) {
        return Err("initialize reply is wrong".into());
    }
    if lines[1].pointer("/result/tools/0/name") != Some(&json!("roundtable")) {
        return Err("tools/list reply is wrong".into());
    }
    Ok(())
}

/// Client config snippets, for `roundtable mcp config --client X` and the app.
pub fn config_snippet(client: &str, engine: &str) -> Option<String> {
    let block = json!({"command": engine, "args": ["mcp", "serve"]});
    Some(match client {
        "" | "json" | "claude" | "claude-app" | "cursor" | "windsurf" | "generic" => serde_json::to_string_pretty(&json!({"mcpServers": {"roundtable": block}})).unwrap(),
        "claude-code" => format!("claude mcp add roundtable --scope user -- {} mcp serve", shell_quote(engine)),
        "codex" => codex_toml(engine),
        "vscode" => serde_json::to_string_pretty(&json!({"servers": {"roundtable": {"type": "stdio", "command": engine, "args": ["mcp", "serve"]}}})).unwrap(),
        _ => return None,
    })
}

/// The Codex CLI table. `tool_timeout_sec` is raised: Codex stops a tool after 60 s by default.
/// `default_tools_approval_mode = "approve"`: the person connected Roundtable on purpose,
/// and without it `codex exec` refuses every call ("requires approval").
pub fn codex_toml(engine: &str) -> String {
    format!(
        "[mcp_servers.roundtable]\ncommand = {}\nargs = [\"mcp\", \"serve\"]\nstartup_timeout_sec = 20\ntool_timeout_sec = {}\ndefault_tools_approval_mode = \"approve\"\n",
        toml_string(engine),
        CODEX_TOOL_TIMEOUT
    )
}

/// Seconds Codex waits for one roundtable (four rounds of slow seats fit).
pub const CODEX_TOOL_TIMEOUT: u64 = 1800;

/// A TOML basic string.
pub fn toml_string(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04X}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn shell_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:".contains(c)) {
        s.to_string()
    } else if cfg!(windows) {
        format!("\"{s}\"")
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(c: &Call, _p: &Progress) -> Outcome {
        let (q, h, r, res, p) = (&c.question, c.heads.as_deref(), c.rounds, c.research, c.personas.as_deref());
        Outcome::text(format!("q={q} h={h:?} r={r:?} res={res} p={p:?}"))
    }

    fn quiet(_: Value) {}

    #[test]
    fn handshake_and_list() {
        let s = Server::new(&fake, false);
        let mut out = Vec::new();
        serve(selftest_input().as_bytes(), &mut out, &s).unwrap();
        selftest_check(&String::from_utf8(out).unwrap()).unwrap();
    }

    #[test]
    fn tool_schema_matches_python_server() {
        let t = tool(false);
        assert_eq!(t["inputSchema"]["required"], json!(["question"]));
        let props: Vec<_> = t["inputSchema"]["properties"].as_object().unwrap().keys().cloned().collect();
        // The Python server's arguments, plus the Rust-only `seats`, `files` and `full_transcript`.
        assert_eq!(props, vec!["question", "heads", "seats", "rounds", "mode", "personas", "research", "coach", "coach_answers", "verdict", "blind", "attach", "decision", "files", "full_transcript"]);
        assert_ne!(tool(true)["inputSchema"]["properties"]["attach"]["description"], t["inputSchema"]["properties"]["attach"]["description"]);
    }

    #[test]
    fn call_passes_arguments() {
        let s = Server::new(&fake, false);
        let m = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","heads":"claude","rounds":2,"research":true,"personas":"pricing"}}});
        let r = s.dispatch(&m, &quiet).unwrap();
        assert_eq!(r["result"]["content"][0]["text"], "q=Hi h=Some(\"claude\") r=Some(2) res=true p=Some(\"pricing\")");
        let m = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","rounds":9}}});
        assert!(s.dispatch(&m, &quiet).unwrap()["result"]["content"][0]["text"].as_str().unwrap().contains("r=Some(4)"));
        assert_eq!(r["result"]["isError"], false);
    }

    #[test]
    fn call_passes_the_new_arguments() {
        let got = Mutex::new(Call::default());
        let run = |c: &Call, _: &Progress| {
            *got.lock().unwrap() = c.clone();
            Outcome::text("")
        };
        let s = Server::new(&run, false);
        let m = json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","coach":true,"coach_answers":["a1","a2"],"verdict":"vote","mode":"deep","blind":false,"attach":"/tmp/x.md","decision":"jebadiah","seats":["ollama:llama3.2","openrouter:openai/gpt-4o-mini"],"files":[{"name":"n.md","text":"notes"}],"full_transcript":true}}});
        s.dispatch(&m, &quiet).unwrap();
        let c = got.lock().unwrap().clone();
        assert!(c.coach);
        assert_eq!(c.coach_answers, vec!["a1", "a2"]);
        assert_eq!(c.verdict.as_deref(), Some("vote"));
        assert_eq!(c.mode.as_deref(), Some("deep"));
        assert_eq!(c.seats.as_deref(), Some("ollama:llama3.2,openrouter:openai/gpt-4o-mini"));
        assert_eq!(c.blind, Some(false));
        assert_eq!(c.attach, vec!["/tmp/x.md"]);
        assert_eq!(c.decision.as_deref(), Some("jebadiah"));
        assert_eq!(c.files, vec![("n.md".to_string(), "notes".to_string())]);
        assert!(c.full_transcript && !c.remote);
        assert_eq!(c.source, "claude");
        assert!(coach_reply(&["Budget?".into()]).ends_with("\n\n1. Budget?"));
    }

    #[test]
    fn remote_takes_links_only_and_marks_the_call() {
        let got = Mutex::new(Call::default());
        let run = |c: &Call, _: &Progress| {
            *got.lock().unwrap() = c.clone();
            Outcome::text("ran")
        };
        let s = Server::new(&run, true);
        let m = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","attach":["/etc/passwd"]}}});
        let r = s.dispatch(&m, &quiet).unwrap();
        assert_eq!(r["error"]["code"], -32602);
        assert!(r["error"]["message"].as_str().unwrap().contains("links only"));
        let m = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","attach":["https://example.com/a"]}}});
        assert_eq!(s.dispatch(&m, &quiet).unwrap()["result"]["content"][0]["text"], "ran");
        let c = got.lock().unwrap().clone();
        assert!(c.remote);
        assert_eq!(c.source, "remote");
        let too_many = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","files":(0..11).map(|i| json!({"name": i.to_string(), "text": "x"})).collect::<Vec<_>>()}}});
        assert_eq!(s.dispatch(&too_many, &quiet).unwrap()["error"]["code"], -32602);
    }

    #[test]
    fn client_name_sets_the_history_source() {
        let got = Mutex::new(String::new());
        let run = |c: &Call, _: &Progress| {
            *got.lock().unwrap() = c.source.clone();
            Outcome::text("")
        };
        let s = Server::new(&run, false);
        s.dispatch(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"codex-mcp-client"}}}), &quiet);
        s.dispatch(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi"}}}), &quiet);
        assert_eq!(*got.lock().unwrap(), "mcp");
    }

    fn a_run(c: &Call, p: &Progress) -> Outcome {
        p(0.0, Some(3.0), "The table sits.");
        p(1.0, None, "Round 1: claude answered.");
        std::thread::sleep(Duration::from_millis(120));
        p(2.0, None, "Round 1: gemini answered.");
        Outcome::Run {
            verdict: if c.question == "no verdict" { None } else { Some("Pick A. Confidence: high.".into()) },
            transcript: "# Roundtable\n\n## claude\nA\n\n## gemini\nA\n".into(),
            answered: 2,
            seats: 2,
            rounds: 1,
            consensus: None,
            history_id: Some("h1".into()),
        }
    }

    #[test]
    fn verdict_first_then_the_transcript_as_a_resource() {
        let s = Server::new(&a_run, false);
        let r = s.dispatch(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"A or B?"}}}), &quiet).unwrap();
        let c = &r["result"]["content"];
        let text = c[0]["text"].as_str().unwrap();
        assert!(text.starts_with("Roundtable: 2 of 2 seats answered, 1 round."), "{text}");
        assert!(text.find("## Verdict").unwrap() < text.find("Pick A").unwrap());
        assert!(!text.contains("## claude"));
        assert_eq!(c[1]["type"], "resource");
        assert_eq!(c[1]["resource"]["uri"], "roundtable://run/h1");
        assert!(c[1]["resource"]["text"].as_str().unwrap().contains("## gemini"));
        assert_eq!(r["result"]["isError"], false);
        // The same transcript through resources/list and resources/read.
        let l = s.dispatch(&json!({"jsonrpc":"2.0","id":2,"method":"resources/list"}), &quiet).unwrap();
        assert_eq!(l["result"]["resources"][0]["uri"], "roundtable://run/h1");
        let rd = s.dispatch(&json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"roundtable://run/h1"}}), &quiet).unwrap();
        assert!(rd["result"]["contents"][0]["text"].as_str().unwrap().contains("## claude"));
        let miss = s.dispatch(&json!({"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"roundtable://run/nope"}}), &quiet).unwrap();
        assert_eq!(miss["error"]["code"], -32002);
        // full_transcript puts it in the text instead.
        let r = s.dispatch(&json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"A or B?","full_transcript":true}}}), &quiet).unwrap();
        assert_eq!(r["result"]["content"].as_array().unwrap().len(), 1);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("## Full transcript"));
        // No verdict: the answers are the reply.
        let r = s.dispatch(&json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"no verdict"}}}), &quiet).unwrap();
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("## claude"));
    }

    #[test]
    fn progress_with_a_token_rises_and_heartbeats() {
        let mut s = Server::new(&a_run, false);
        s.heartbeat = Duration::from_millis(30);
        let sent = Mutex::new(Vec::<Value>::new());
        let notify = |v: Value| sent.lock().unwrap().push(v);
        let r = s.dispatch(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"A?"},"_meta":{"progressToken":"t1"}}}), &notify).unwrap();
        assert!(r["result"].is_object());
        let sent = sent.lock().unwrap();
        assert!(sent.len() >= 4, "{sent:?}");
        let mut last = -1.0;
        for n in sent.iter() {
            assert_eq!(n["method"], "notifications/progress");
            assert_eq!(n["params"]["progressToken"], "t1");
            let p = n["params"]["progress"].as_f64().unwrap();
            assert!(p > last, "progress must rise: {sent:?}");
            last = p;
        }
        assert!(sent.iter().any(|n| n["params"]["message"].as_str().unwrap().contains("Still working")));
        // Without a token, no notifications.
        let none = Mutex::new(0);
        s.dispatch(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"A?"}}}), &|_| *none.lock().unwrap() += 1);
        assert_eq!(*none.lock().unwrap(), 0);
    }

    #[test]
    fn stdio_progress_lines_come_before_the_reply() {
        let s = Server::new(&a_run, false);
        let input = json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"A?"},"_meta":{"progressToken":7}}}).to_string() + "\n";
        let mut out = Vec::new();
        serve(input.as_bytes(), &mut out, &s).unwrap();
        let lines: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert!(lines.len() >= 4);
        assert!(lines[..lines.len() - 1].iter().all(|l| l["method"] == "notifications/progress"));
        assert_eq!(lines.last().unwrap()["id"], 9);
    }

    #[test]
    fn errors() {
        let s = Server::new(&fake, false);
        let m = json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"roundtable","arguments":{}}});
        assert_eq!(s.dispatch(&m, &quiet).unwrap()["error"]["code"], -32602);
        let m = json!({"jsonrpc":"2.0","id":5,"method":"nope"});
        assert_eq!(s.dispatch(&m, &quiet).unwrap()["error"]["code"], -32601);
        let m = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert!(s.dispatch(&m, &quiet).is_none());
        let mut out = Vec::new();
        serve("not json\n".as_bytes(), &mut out, &s).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("-32700"));
    }

    #[test]
    fn snippets() {
        let e = "/Users/a b/rt";
        let v: Value = serde_json::from_str(&config_snippet("cursor", e).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["roundtable"]["command"], e);
        let t = config_snippet("codex", e).unwrap();
        assert!(t.starts_with("[mcp_servers.roundtable]\ncommand = \"/Users/a b/rt\"\n"));
        assert!(t.contains("tool_timeout_sec = 1800"));
        assert!(config_snippet("claude-code", e).unwrap().contains("'/Users/a b/rt' mcp serve") || cfg!(windows));
        assert_eq!(toml_string("C:\\x\"y"), "\"C:\\\\x\\\"y\"");
        assert!(config_snippet("nope", e).is_none());
    }
}
