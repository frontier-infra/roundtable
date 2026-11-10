//! Stdio MCP server: the same single `roundtable` tool and schema as `lib/mcp.py`.
//! Newline-delimited JSON-RPC 2.0. Stdout carries only MCP messages.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub fn tool() -> Value {
    json!({
        "name": "roundtable",
        "description": "Convene a council of frontier AI models (Grok, Codex/OpenAI, GLM, MiniMax, Claude, Gemini) on a question and return their answers as a markdown transcript. With rounds>1 the council deliberates across rounds until a Claude chair declares consensus. Heads with no configured key/CLI are skipped gracefully.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "The question to put to the council."},
                "heads": {"type": "string", "description": "Optional comma-separated subset of heads to convene (any of: grok,codex,openai,glm,minimax,claude,gemini). Defaults to all configured heads."},
                "rounds": {"type": "integer", "description": "Optional number of rounds, 1 to 4. 1 (default) = advisory (blind, independent answers); 2 to 4 = the seats read each other and revise toward consensus, and a Claude chair gives the verdict, stopping early when they agree.", "minimum": 1, "maximum": 4},
                "personas": {"type": "string", "description": "Optional seat roles. A preset (security-review, go-to-market, hiring-decision, pricing, architecture-review, exec-team, board-review, frontier-four), a pack (business-roles, frontier-labs), or a comma list of persona ids or names, each optionally ending in @<head> (cfo,ceo@claude). 'generic' or empty = plain seats. Lab seats are simulated perspectives drawn from public positions, not affiliated with or endorsed by the lab."},
                "research": {"type": "boolean", "description": "Optional. Enable web research / deeper reasoning for heads that support it (longer per-head timeout). Defaults to false."}
            },
            "required": ["question"],
            "additionalProperties": false
        }
    })
}

/// One tool call: (question, heads, rounds, research, personas) -> (text, is_error).
pub type Runner<'a> = dyn Fn(&str, Option<&str>, Option<u32>, bool, Option<&str>) -> (String, bool) + 'a;

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
            let heads = args.get("heads").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty());
            let research = args.get("research").and_then(|v| v.as_bool()).unwrap_or(false);
            let personas = args.get("personas").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty());
            let (text, is_error) = run(&q, heads, rounds, research, personas);
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

    fn fake(q: &str, h: Option<&str>, r: Option<u32>, res: bool, p: Option<&str>) -> (String, bool) {
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
        assert_eq!(props, vec!["question", "heads", "rounds", "personas", "research"]);
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
