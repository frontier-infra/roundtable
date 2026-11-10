//! Streamable HTTP transport for the MCP server: `roundtable mcp serve --http`.
//!
//! - One endpoint, `/mcp`. POST carries JSON-RPC; GET answers 405 (no server-initiated
//!   stream), DELETE ends a session.
//! - Every request needs the token the person controls: `Authorization: Bearer <token>`,
//!   or the token as the last path part, `/mcp/<token>`, for clients that cannot send a
//!   header (ChatGPT and Claude custom connectors take a URL and OAuth or no auth).
//! - Binds 127.0.0.1 by default. A public URL comes from a tunnel the app starts.
//! - A `tools/call` that accepts `text/event-stream` gets an SSE stream: progress
//!   notifications, a keepalive comment every 15 s, then the result.
//! - Rate limits: roundtables a minute, roundtables at once, requests a minute.
//! - Only the roundtable tool and its own transcripts. No shell, no file reads:
//!   the remote `Server` takes links to public addresses and inline text only.

use crate::mcp::{err, Server};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, StatusCode};

/// The largest request body: two million characters of files fit with room to spare.
pub const MAX_BODY: usize = 8 * 1024 * 1024;
const KEEPALIVE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub token: String,
    /// Roundtables started per minute.
    pub calls_per_minute: usize,
    /// Roundtables running at once.
    pub max_running: usize,
    /// All requests per minute.
    pub requests_per_minute: usize,
    /// Browser origins allowed besides the AI vendors' own.
    pub allowed_origins: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config { host: "127.0.0.1".into(), port: 8765, token: String::new(), calls_per_minute: 6, max_running: 2, requests_per_minute: 120, allowed_origins: Vec::new() }
    }
}

/// A fresh token: 32 random bytes, URL-safe.
pub fn new_token() -> String {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).expect("the system random source failed");
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join("")
}

/// Compare without stopping at the first difference.
fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A window of times, for "at most N a minute".
struct Window {
    limit: usize,
    times: Mutex<VecDeque<Instant>>,
}

impl Window {
    fn new(limit: usize) -> Self {
        Window { limit, times: Mutex::new(VecDeque::new()) }
    }
    fn take(&self) -> bool {
        let mut t = self.times.lock().unwrap();
        let now = Instant::now();
        while t.front().is_some_and(|f| now.duration_since(*f) > Duration::from_secs(60)) {
            t.pop_front();
        }
        if t.len() >= self.limit {
            return false;
        }
        t.push_back(now);
        true
    }
}

/// Origins that may call from a browser: the vendors' own sites, plus any the person adds.
fn origin_ok(origin: &str, extra: &[String]) -> bool {
    let o = origin.trim().trim_end_matches('/').to_ascii_lowercase();
    if extra.iter().any(|e| e.trim().trim_end_matches('/').eq_ignore_ascii_case(&o)) {
        return true;
    }
    let host = o.strip_prefix("https://").unwrap_or("");
    ["chatgpt.com", "openai.com", "claude.ai", "anthropic.com", "claude.com"].iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// One logged request, for the app's call list and the terminal.
pub type Log<'a> = dyn Fn(Value) + Sync + 'a;

struct State<'s, 'a> {
    server: &'s Server<'a>,
    cfg: &'s Config,
    calls: Window,
    requests: Window,
    running: AtomicUsize,
    log: &'s Log<'s>,
}

fn header(req: &Request, name: &str) -> Option<String> {
    req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_string())
}

fn hdr(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn json_response(status: u16, body: &Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(serde_json::to_vec(body).unwrap_or_default()).with_status_code(StatusCode(status)).with_header(hdr("Content-Type", "application/json"))
}

fn plain(status: u16, text: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(text.as_bytes().to_vec()).with_status_code(StatusCode(status)).with_header(hdr("Content-Type", "text/plain; charset=utf-8"))
}

/// The SSE body: events from the run, a keepalive comment while it is quiet.
struct Stream {
    rx: mpsc::Receiver<String>,
    buf: Vec<u8>,
    pos: usize,
    done: bool,
}

impl Read for Stream {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.pos >= self.buf.len() {
            if self.done {
                return Ok(0);
            }
            match self.rx.recv_timeout(KEEPALIVE) {
                Ok(s) => self.buf = s.into_bytes(),
                Err(mpsc::RecvTimeoutError::Timeout) => self.buf = b": keepalive\n\n".to_vec(),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.done = true;
                    return Ok(0);
                }
            }
            self.pos = 0;
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

fn sse(v: &Value) -> String {
    format!("event: message\ndata: {}\n\n", serde_json::to_string(v).unwrap_or_default())
}

/// The token in the path, `/mcp/<token>`, if any. None: the path is not ours.
fn route(url: &str) -> Option<Option<String>> {
    let path = url.split('?').next().unwrap_or("").trim_end_matches('/');
    if path == "/mcp" {
        return Some(None);
    }
    path.strip_prefix("/mcp/").filter(|t| !t.is_empty() && !t.contains('/')).map(|t| Some(t.to_string()))
}

fn handle(st: &State, mut req: Request) {
    let started = Instant::now();
    let peer = header(&req, "CF-Connecting-IP").or_else(|| req.remote_addr().map(|a| a.ip().to_string())).unwrap_or_default();
    let agent = header(&req, "User-Agent").unwrap_or_default().chars().take(80).collect::<String>();
    let log = |status: u16, method: &str, extra: Value| {
        let mut v = json!({"type": "request", "ts": crate::history::now(), "status": status, "method": method, "peer": peer, "agent": agent, "ms": started.elapsed().as_millis() as u64});
        if let (Some(o), Value::Object(e)) = (v.as_object_mut(), extra) {
            o.extend(e);
        }
        (st.log)(v);
    };
    let Some(path_token) = route(req.url()) else {
        let _ = req.respond(plain(404, "Not found. The MCP endpoint is /mcp."));
        return;
    };
    // The token first: nothing else is said to a caller without it.
    let bearer = header(&req, "Authorization").and_then(|a| a.strip_prefix("Bearer ").or_else(|| a.strip_prefix("bearer ")).map(|t| t.trim().to_string()));
    let given = path_token.or(bearer).unwrap_or_default();
    if st.cfg.token.is_empty() || !same(&given, &st.cfg.token) {
        log(401, "", json!({"note": "wrong or missing token"}));
        let _ = req.respond(plain(401, "Roundtable needs its token.").with_header(hdr("WWW-Authenticate", "Bearer realm=\"roundtable\"")));
        return;
    }
    if let Some(o) = header(&req, "Origin") {
        if !origin_ok(&o, &st.cfg.allowed_origins) {
            log(403, "", json!({"note": format!("origin {o} not allowed")}));
            let _ = req.respond(plain(403, "That origin may not call Roundtable."));
            return;
        }
    }
    if !st.requests.take() {
        log(429, "", json!({"note": "too many requests"}));
        let _ = req.respond(json_response(429, &err(&Value::Null, -32000, "Too many requests. Wait a minute.")).with_header(hdr("Retry-After", "60")));
        return;
    }
    match req.method() {
        Method::Post => {}
        Method::Delete => {
            let _ = req.respond(plain(200, ""));
            return;
        }
        _ => {
            let _ = req.respond(plain(405, "Use POST.").with_header(hdr("Allow", "POST, DELETE")));
            return;
        }
    }
    let mut body = Vec::new();
    if req.as_reader().take(MAX_BODY as u64 + 1).read_to_end(&mut body).is_err() || body.len() > MAX_BODY {
        log(413, "", json!({"note": "request too large"}));
        let _ = req.respond(plain(413, "The request is too large."));
        return;
    }
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            let _ = req.respond(json_response(400, &err(&Value::Null, -32700, "Parse error")));
            return;
        }
    };
    if msg.is_array() {
        let _ = req.respond(json_response(400, &err(&Value::Null, -32600, "Batches are not supported. Send one message per request.")));
        return;
    }
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
    // A notification or a response: accepted, nothing to say.
    if msg.get("id").is_none() || method.is_empty() {
        let _ = req.respond(Response::empty(StatusCode(202)));
        return;
    }
    let mut session = Vec::new();
    if method == "initialize" {
        session.push(hdr("Mcp-Session-Id", &new_token()[..32]));
    }
    if method != "tools/call" {
        let reply = st.server.dispatch(&msg, &|_| {}).unwrap_or(Value::Null);
        log(200, &method, json!({}));
        let mut r = json_response(200, &reply);
        for h in session {
            r.add_header(h);
        }
        let _ = req.respond(r);
        return;
    }
    // A roundtable: the rate limits, then the run.
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let question: String = msg.pointer("/params/arguments/question").and_then(|q| q.as_str()).unwrap_or("").chars().take(200).collect();
    if !st.calls.take() {
        log(429, &method, json!({"question": question, "note": "rate limit"}));
        let m = format!("Rate limit: at most {} roundtables a minute. Try again in a minute.", st.cfg.calls_per_minute);
        let _ = req.respond(json_response(429, &err(&id, -32000, &m)).with_header(hdr("Retry-After", "60")));
        return;
    }
    if st.running.fetch_add(1, Ordering::SeqCst) >= st.cfg.max_running {
        st.running.fetch_sub(1, Ordering::SeqCst);
        log(429, &method, json!({"question": question, "note": "busy"}));
        let m = format!("Roundtable is already running {} roundtables. Try again when one finishes.", st.cfg.max_running);
        let _ = req.respond(json_response(429, &err(&id, -32000, &m)).with_header(hdr("Retry-After", "30")));
        return;
    }
    (st.log)(json!({"type": "call_started", "ts": crate::history::now(), "question": question, "peer": peer, "agent": agent}));
    let wants_stream = header(&req, "Accept").is_some_and(|a| a.contains("text/event-stream"));
    let summary = |reply: &Value| -> Value {
        let text = reply.pointer("/result/content/0/text").and_then(|t| t.as_str()).unwrap_or("");
        json!({"question": question, "is_error": reply.pointer("/result/isError").and_then(|e| e.as_bool()).unwrap_or(reply.get("error").is_some()), "result": text.lines().next().unwrap_or("").chars().take(160).collect::<String>(), "error": reply.pointer("/error/message")})
    };
    if wants_stream {
        let (tx, rx) = mpsc::channel::<String>();
        std::thread::scope(|s| {
            let tx2 = tx.clone();
            let worker = s.spawn(move || {
                let notify = |v: Value| {
                    let _ = tx2.send(sse(&v));
                };
                let reply = st.server.dispatch(&msg, &notify).unwrap_or(Value::Null);
                let _ = tx2.send(sse(&reply));
                reply
            });
            drop(tx);
            let body = Stream { rx, buf: Vec::new(), pos: 0, done: false };
            let mut r = Response::new(StatusCode(200), vec![hdr("Content-Type", "text/event-stream"), hdr("Cache-Control", "no-cache"), hdr("X-Accel-Buffering", "no")], body, None, None);
            for h in session {
                r.add_header(h);
            }
            let _ = req.respond(r);
            let reply = worker.join().unwrap_or(Value::Null);
            st.running.fetch_sub(1, Ordering::SeqCst);
            log(200, &method, summary(&reply));
        });
    } else {
        let reply = st.server.dispatch(&msg, &|_| {}).unwrap_or(Value::Null);
        st.running.fetch_sub(1, Ordering::SeqCst);
        log(200, &method, summary(&reply));
        let _ = req.respond(json_response(200, &reply));
    }
}

/// Serve until `stop` is set. `ready` gets the bound address (port 0 picks a free one).
pub fn serve(server: &Server, cfg: &Config, stop: &AtomicBool, ready: &dyn Fn(std::net::SocketAddr), log: &Log) -> Result<(), String> {
    if cfg.token.len() < 16 {
        return Err("The token must be at least 16 characters.".into());
    }
    let http = tiny_http::Server::http((cfg.host.as_str(), cfg.port)).map_err(|e| format!("Could not listen on {}:{}: {e}", cfg.host, cfg.port))?;
    let addr = http.server_addr().to_ip().ok_or("Not an IP address.")?;
    ready(addr);
    let st = State { server, cfg, calls: Window::new(cfg.calls_per_minute.max(1)), requests: Window::new(cfg.requests_per_minute.max(1)), running: AtomicUsize::new(0), log };
    std::thread::scope(|s| {
        while !stop.load(Ordering::SeqCst) {
            match http.recv_timeout(Duration::from_millis(250)) {
                Ok(Some(req)) => {
                    let st = &st;
                    s.spawn(move || handle(st, req));
                }
                Ok(None) => {}
                Err(_) => break,
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{Call, Outcome, Progress};
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;

    fn post(addr: std::net::SocketAddr, path: &str, headers: &[(&str, &str)], body: &str) -> (u16, String, String) {
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        let mut h = format!("POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
        for (k, v) in headers {
            h.push_str(&format!("{k}: {v}\r\n"));
        }
        h.push_str("\r\n");
        s.write_all(h.as_bytes()).unwrap();
        s.write_all(body.as_bytes()).unwrap();
        let mut r = BufReader::new(s);
        let mut status = String::new();
        r.read_line(&mut status).unwrap();
        let code: u16 = status.split_whitespace().nth(1).unwrap().parse().unwrap();
        let mut head = String::new();
        loop {
            let mut l = String::new();
            r.read_line(&mut l).unwrap();
            if l == "\r\n" || l.is_empty() {
                break;
            }
            head.push_str(&l);
        }
        let mut rest = String::new();
        let _ = r.read_to_string(&mut rest);
        (code, head, rest)
    }

    fn slow(c: &Call, p: &Progress) -> Outcome {
        p(0.0, Some(2.0), "The table sits.");
        std::thread::sleep(Duration::from_millis(150));
        p(1.0, None, "claude answered.");
        Outcome::Run { verdict: Some(format!("Verdict for {}", c.question)), transcript: "## claude\nyes".into(), answered: 1, seats: 1, rounds: 1, consensus: None, history_id: None }
    }

    const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#;
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn with_server(cfg: Config, f: impl FnOnce(std::net::SocketAddr, &Mutex<Vec<Value>>)) {
        let server = Server::new(&slow, true);
        let stop = AtomicBool::new(false);
        let logs = Mutex::new(Vec::new());
        let (tx, rx) = mpsc::channel();
        std::thread::scope(|s| {
            let (server, stop, logs, cfg) = (&server, &stop, &logs, &cfg);
            s.spawn(move || {
                serve(server, cfg, stop, &|a| tx.send(a).unwrap(), &|v| logs.lock().unwrap().push(v)).unwrap();
            });
            let addr = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            f(addr, logs);
            stop.store(true, Ordering::SeqCst);
        });
    }

    fn cfg() -> Config {
        Config { port: 0, token: TOKEN.into(), ..Default::default() }
    }

    #[test]
    fn token_by_header_or_path_and_nothing_without_it() {
        with_server(cfg(), |a, logs| {
            assert_eq!(post(a, "/mcp", &[], INIT).0, 401);
            assert_eq!(post(a, "/mcp", &[("Authorization", "Bearer wrong-token-wrong-token-wrong-to")], INIT).0, 401);
            assert_eq!(post(a, "/mcp/nope", &[], INIT).0, 401);
            assert_eq!(post(a, "/other", &[], INIT).0, 404);
            let (c, head, body) = post(a, "/mcp", &[("Authorization", &format!("Bearer {TOKEN}"))], INIT);
            assert_eq!(c, 200);
            assert!(head.to_ascii_lowercase().contains("mcp-session-id"));
            assert!(body.contains("\"serverInfo\""));
            let (c, _, body) = post(a, &format!("/mcp/{TOKEN}"), &[], r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
            assert_eq!(c, 200);
            assert!(body.contains("links only") || body.contains("public addresses only"));
            assert_eq!(post(a, &format!("/mcp/{TOKEN}"), &[], r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).0, 202);
            assert!(logs.lock().unwrap().iter().any(|l| l["status"] == 401));
        });
    }

    #[test]
    fn origins_from_unknown_sites_are_refused() {
        with_server(cfg(), |a, _| {
            let auth = format!("Bearer {TOKEN}");
            assert_eq!(post(a, "/mcp", &[("Authorization", &auth), ("Origin", "https://evil.example")], INIT).0, 403);
            assert_eq!(post(a, "/mcp", &[("Authorization", &auth), ("Origin", "https://chatgpt.com")], INIT).0, 200);
        });
    }

    #[test]
    fn a_call_streams_progress_then_the_verdict() {
        with_server(cfg(), |a, logs| {
            let call = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"A or B?"},"_meta":{"progressToken":"p"}}}"#;
            let (c, head, body) = post(a, &format!("/mcp/{TOKEN}"), &[("Accept", "application/json, text/event-stream")], call);
            assert_eq!(c, 200);
            assert!(head.to_ascii_lowercase().contains("text/event-stream"));
            let events: Vec<Value> = body.lines().filter_map(|l| l.strip_prefix("data: ")).map(|d| serde_json::from_str(d).unwrap()).collect();
            assert!(events.len() >= 3, "{body}");
            assert!(events.iter().any(|e| e["method"] == "notifications/progress"));
            let last = events.last().unwrap();
            assert_eq!(last["id"], 5);
            assert!(last["result"]["content"][0]["text"].as_str().unwrap().contains("Verdict for A or B?"));
            // Plain JSON when the client does not take a stream.
            let (c, _, body) = post(a, &format!("/mcp/{TOKEN}"), &[("Accept", "application/json")], call);
            assert_eq!(c, 200);
            assert!(body.contains("Verdict for A or B?"));
            let l = logs.lock().unwrap();
            assert!(l.iter().any(|x| x["type"] == "call_started" && x["question"] == "A or B?"));
            assert!(l.iter().any(|x| x["method"] == "tools/call" && x["status"] == 200 && x["is_error"] == false));
        });
    }

    #[test]
    fn rate_limit_answers_429() {
        with_server(Config { calls_per_minute: 1, ..cfg() }, |a, _| {
            let call = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Q"}}}"#;
            assert_eq!(post(a, &format!("/mcp/{TOKEN}"), &[], call).0, 200);
            let (c, _, body) = post(a, &format!("/mcp/{TOKEN}"), &[], call);
            assert_eq!(c, 429);
            assert!(body.contains("Rate limit"));
        });
    }

    #[test]
    fn remote_refuses_local_paths() {
        with_server(cfg(), |a, _| {
            let call = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Q","attach":["/etc/passwd"]}}}"#;
            let (_, _, body) = post(a, &format!("/mcp/{TOKEN}"), &[], call);
            assert!(body.contains("links only"), "{body}");
        });
    }

    #[test]
    fn short_tokens_are_refused_and_tokens_are_long() {
        let server = Server::new(&slow, true);
        let e = serve(&server, &Config { token: "short".into(), ..cfg() }, &AtomicBool::new(true), &|_| {}, &|_| {}).unwrap_err();
        assert!(e.contains("16"));
        assert_eq!(new_token().len(), 64);
        assert_ne!(new_token(), new_token());
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
        assert_eq!(route("/mcp"), Some(None));
        assert_eq!(route("/mcp/"), Some(None));
        assert_eq!(route("/mcp/tok?x=1"), Some(Some("tok".into())));
        assert_eq!(route("/mcp/a/b"), None);
        assert_eq!(route("/"), None);
    }
}
