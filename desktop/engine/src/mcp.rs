//! Stdio MCP server: the same single `roundtable` tool and schema as `lib/mcp.py`.
//! Newline-delimited JSON-RPC 2.0. Stdout carries only MCP messages.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub fn tool() -> Value {
    json!({
        "name": "roundtable",
        "description": "Convene a council of frontier AI models (Grok, Codex/OpenAI, GLM, MiniMax, Claude, Gemini) on a question and return their answers as a markdown transcript. With rounds>1 the council deliberates across rounds until a Claude chair declares consensus. Heads with no configured key/CLI are skipped gracefully. Roundtable is free from Titanium Computing (https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=mcp).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "The question to put to the council."},
                "heads": {"type": "string", "description": "Optional comma-separated subset of heads to convene (any of: grok,codex,openai,glm,minimax,claude,gemini). Defaults to all configured heads."},
                "seats": {"description": "Optional. Seats from providers added in the Roundtable app or with `roundtable provider add` (OpenRouter, Ollama, LM Studio, Groq, Together, any OpenAI-compatible URL), as provider:model, for example ollama:llama3.2 or openrouter:openai/gpt-4o-mini. A comma-separated string or an array. Alone, the table is exactly these seats; with heads, they are added.", "anyOf": [{"type": "string"}, {"type": "array", "items": {"type": "string"}}]},
                "rounds": {"type": "integer", "description": "Optional number of rounds, 1 to 4. 1 (default) = advisory (blind, independent answers); 2 to 4 = the seats read each other and revise toward consensus, and a Claude chair gives the verdict, stopping early when they agree.", "minimum": 1, "maximum": 4},
                "mode": {"type": "string", "enum": ["quick", "deep"], "description": "Optional. quick (the default): one round, short seat answers and a short verdict (the pick, confidence, what would change it, three next steps, one minority view), about 2 minutes. deep: Deep Think, for complex asks (several stakeholders, money or legal stakes, more than three options, missing facts): the coach first, up to 4 rounds that argue against the leading pick, long seat answers and a long verdict (reasoning by theme, trade-offs, risks, unknowns and how to resolve them, a step plan, the minority view in full). It takes about 10 to 15 minutes. rounds, when given, overrides the mode's rounds."},
                "personas": {"type": "string", "description": "Optional seat roles. A preset (security-review, go-to-market, hiring-decision, pricing, architecture-review, exec-team, board-review, frontier-four), a pack (business-roles, frontier-labs), or a comma list of persona ids or names, each optionally ending in @<head> (cfo,ceo@claude). 'generic' or empty = plain seats. Lab seats are simulated perspectives drawn from public positions, not affiliated with or endorsed by the lab."},
                "research": {"type": "boolean", "description": "Optional. Enable web research / deeper reasoning for heads that support it (longer per-head timeout). Defaults to false."},
                "coach": {"type": "boolean", "description": "Optional. The question coach: one AI (the chair's seat) asks at most three clarifying questions, then writes a brief (goal, context, constraints, what a good answer looks like) that the table reads. Without coach_answers, the tool returns the questions; ask the person, then call again with the same question, coach true, and coach_answers."},
                "coach_answers": {"type": "array", "items": {"type": "string"}, "description": "Optional. The answers to the coach's questions, in order."},
                "verdict": {"type": "string", "enum": ["decision", "pros-cons", "plan", "vote", "none"], "description": "Optional. The chair's final verdict shape: a decision (the default with two or more seats), pros and cons, an action plan, a vote, or none. Every verdict leads with the pick, its confidence, what would change it and three next steps, and names the minority view and who held it. It opens with QUESTIONS FIRST when the question lacks key facts."},
                "blind": {"type": "boolean", "description": "Optional. Round 1 is blind (true, the default): seats answer without seeing each other. false lets each seat in round 1 read the seats before it."},
                "attach": {"type": "array", "items": {"type": "string"}, "description": "Optional. Paths of text or PDF files, or http(s) links, that every seat reads. Each is read once; a long one is summarized once and the summary shared."},
                "decision": {"type": "string", "description": "Optional. After the final round, a decision model rates the options the table argued (plus 'need more information') with calibrated probabilities: jev (TypeSafe, needs a key), jebadiah (local Jebadiah), systemone (another System One model set in Settings, for example LINK21), or off. Defaults to the saved setting (off unless chosen).", "enum": ["jev", "jebadiah", "systemone", "link21", "off"]}
            },
            "required": ["question"],
            "additionalProperties": false
        }
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
}

/// One tool call -> (text, is_error).
pub type Runner<'a> = dyn Fn(&Call) -> (String, bool) + 'a;

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
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        _ => Vec::new(),
    }
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Route one message. Notifications get `None`.
pub fn dispatch(msg: &Value, run: &Runner) -> Option<Value> {
    let Some(obj) = msg.as_object() else { return None };
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
            ok(&id, json!({
                "protocolVersion": v,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "roundtable", "version": crate::VERSION},
                "instructions": "Call the 'roundtable' tool with a question to convene a council of frontier models. Use rounds>1 for chaired deliberation to consensus."
            }))
        }
        "tools/list" => ok(&id, json!({"tools": [tool()]})),
        "ping" => ok(&id, json!({})),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name != "roundtable" {
                return Some(err(&id, -32602, &format!("Unknown tool: '{name}'")));
            }
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let q = args.get("question").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if q.is_empty() {
                return Some(err(&id, -32602, "Missing required argument: 'question' (non-empty string)"));
            }
            let rounds = match args.get("rounds") {
                None | Some(Value::Null) => None,
                Some(v) => match v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())) {
                    Some(n) => Some(n.clamp(1, 4) as u32),
                    None => return Some(err(&id, -32602, "'rounds' must be an integer")),
                },
            };
            let text_arg = |k: &str| args.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
            let call = Call {
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
                attach: strings(args.get("attach")),
                decision: text_arg("decision"),
            };
            let (text, is_error) = run(&call);
            ok(&id, json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
        }
        other => err(&id, -32601, &format!("Method not found: {other}")),
    })
}

pub fn serve<R: BufRead, W: Write>(input: R, mut output: W, run: &Runner) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => dispatch(&msg, run),
            Err(_) => Some(err(&Value::Null, -32700, "Parse error")),
        };
        if let Some(r) = reply {
            writeln!(output, "{}", serde_json::to_string(&r)?)?;
            output.flush()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(c: &Call) -> (String, bool) {
        let (q, h, r, res, p) = (&c.question, c.heads.as_deref(), c.rounds, c.research, c.personas.as_deref());
        (format!("q={q} h={h:?} r={r:?} res={res} p={p:?}"), false)
    }

    #[test]
    fn handshake_and_list() {
        let mut out = Vec::new();
        serve(selftest_input().as_bytes(), &mut out, &fake).unwrap();
        selftest_check(&String::from_utf8(out).unwrap()).unwrap();
    }

    #[test]
    fn tool_schema_matches_python_server() {
        let t = tool();
        assert_eq!(t["inputSchema"]["required"], json!(["question"]));
        let props: Vec<_> = t["inputSchema"]["properties"].as_object().unwrap().keys().cloned().collect();
        // The Python server's arguments, plus `seats` (provider seats live in the Rust engine only).
        assert_eq!(props, vec!["question", "heads", "seats", "rounds", "mode", "personas", "research", "coach", "coach_answers", "verdict", "blind", "attach", "decision"]);
    }

    #[test]
    fn call_passes_arguments() {
        let m = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","heads":"claude","rounds":2,"research":true,"personas":"pricing"}}});
        let r = dispatch(&m, &fake).unwrap();
        assert_eq!(r["result"]["content"][0]["text"], "q=Hi h=Some(\"claude\") r=Some(2) res=true p=Some(\"pricing\")");
        let m = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","rounds":9}}});
        assert!(dispatch(&m, &fake).unwrap()["result"]["content"][0]["text"].as_str().unwrap().contains("r=Some(4)"));
        assert_eq!(r["result"]["isError"], false);
    }

    #[test]
    fn call_passes_the_new_arguments() {
        let got = std::sync::Mutex::new(Call::default());
        let run = |c: &Call| {
            *got.lock().unwrap() = c.clone();
            (String::new(), false)
        };
        let m = json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Hi","coach":true,"coach_answers":["a1","a2"],"verdict":"vote","mode":"deep","blind":false,"attach":"/tmp/x.md","decision":"jebadiah","seats":["ollama:llama3.2","openrouter:openai/gpt-4o-mini"]}}});
        dispatch(&m, &run).unwrap();
        let c = got.lock().unwrap().clone();
        assert!(c.coach);
        assert_eq!(c.coach_answers, vec!["a1", "a2"]);
        assert_eq!(c.verdict.as_deref(), Some("vote"));
        assert_eq!(c.mode.as_deref(), Some("deep"));
        assert_eq!(c.seats.as_deref(), Some("ollama:llama3.2,openrouter:openai/gpt-4o-mini"));
        assert_eq!(c.blind, Some(false));
        assert_eq!(c.attach, vec!["/tmp/x.md"]);
        assert_eq!(c.decision.as_deref(), Some("jebadiah"));
        assert!(coach_reply(&["Budget?".into()]).ends_with("\n\n1. Budget?"));
    }

    #[test]
    fn errors() {
        let m = json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"roundtable","arguments":{}}});
        assert_eq!(dispatch(&m, &fake).unwrap()["error"]["code"], -32602);
        let m = json!({"jsonrpc":"2.0","id":5,"method":"nope"});
        assert_eq!(dispatch(&m, &fake).unwrap()["error"]["code"], -32601);
        let m = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert!(dispatch(&m, &fake).is_none());
        let mut out = Vec::new();
        serve("not json\n".as_bytes(), &mut out, &fake).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("-32700"));
    }
}
