//! The real engine binary against fake AI tools on PATH, one per provider.
//! Each fake checks the argument shape its real tool needs (taken from that tool's
//! own --help) and exits with an error when it is wrong, so a flag drift fails here.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

fn script(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Fakes that answer "<tool> ok" only when called the way the real tool needs.
fn fakes(bin: &Path) {
    // Prompt on stdin, print mode, never load MCP servers.
    script(bin, "claude", r#"q=$(cat); case " $* " in *" -p "*"--strict-mcp-config"*) echo "claude ok: $q";; *) echo "bad claude args: $*" >&2; exit 9;; esac"#);
    // Answer goes to the -o file.
    script(
        bin,
        "codex",
        r#"[ "$1" = exec ] || { echo "bad codex args" >&2; exit 9; }
out=""; while [ $# -gt 0 ]; do [ "$1" = "-o" ] && out="$2"; shift; done
[ -n "$out" ] && echo "codex ok" > "$out""#,
    );
    // A fresh folder is untrusted: without --skip-trust gemini exits 55.
    script(bin, "gemini", r#"case " $* " in *" --skip-trust "*) q=$(cat); echo "gemini ok: $q";; *) echo "Gemini CLI is not running in a trusted directory." >&2; exit 55;; esac"#);
    // -p PROMPT, no fixed model.
    script(bin, "grok", r#"[ "$1" = "-p" ] || exit 9; case " $* " in *" -m "*) echo "Couldn't set model: unknown model id" >&2; exit 1;; esac; echo "grok ok: $2""#);
    script(bin, "qwen", r#"q=$(cat); case " $* " in *" --output-format text "*) echo "qwen ok: $q";; *) exit 9;; esac"#);
    // kimi 2.5 refuses --quiet, and --plan with --prompt.
    script(
        bin,
        "kimi",
        r#"for a in "$@"; do case "$a" in --quiet) echo "error: unknown option '--quiet'" >&2; exit 1;; --plan) echo "error: Cannot combine --prompt with --plan." >&2; exit 1;; esac; done
[ "$1" = "-p" ] && echo "kimi ok: $2""#,
    );
    // The default model's provider is down; a signed-in provider's model works.
    script(
        bin,
        "opencode",
        r#"case "$1 $2" in "auth list") echo "Z.AI Coding Plan  Z.AI Coding Plan   stored"; exit 0;; models*) printf 'hardware-box/default\nzai-coding-plan/glm-5.3\n'; exit 0;; esac
[ "$1" = run ] || exit 9
[ "$2" = "-m" ] && [ "$3" = "zai-coding-plan/glm-5.3" ] && { echo "opencode ok: $4"; exit 0; }
echo "Error: Provider request failed with HTTP 405: Method Not Allowed" >&2; exit 1"#,
    );
    script(bin, "cursor-agent", r#"case " $* " in *" -p "*"--trust"*) echo "Error: Authentication required. Please run 'agent login' first." >&2; exit 1;; *) exit 9;; esac"#);
}

fn engine() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_roundtable-engine"))
}

fn run(home: &Path, bin: &Path, args: &[&str], extra: &[(&str, &str)]) -> (i32, String) {
    let mut c = Command::new(engine());
    c.args(args)
        .env_clear()
        .env("HOME", home)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("TMPDIR", std::env::temp_dir())
        .env("ROUNDTABLE_KEYSTORE", "memory")
        .env("ROUNDTABLE_DATA_DIR", home.join("data"))
        .env("ROUNDTABLE_PATH_ONLY", "1")
        .stdin(Stdio::null());
    for (k, v) in extra {
        c.env(k, v);
    }
    let o = c.output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into_owned())
}

fn test_json(home: &Path, bin: &Path, head: &str, extra: &[(&str, &str)]) -> serde_json::Value {
    let (_, out) = run(home, bin, &["test", head, "--json", "--timeout", "20"], extra);
    serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("{head}: {e}: {out}"))
}

#[test]
fn every_tool_is_called_the_way_it_needs() {
    let home = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    fakes(bin.path());
    let p = "Answer in five words: is water wet?";
    for (head, want) in [
        ("claude", format!("claude ok: {p}")),
        ("codex", "codex ok".to_string()),
        ("gemini", format!("gemini ok: {p}")),
        ("grok", format!("grok ok: {p}")),
        ("qwen", format!("qwen ok: {p}")),
        ("kimi", format!("kimi ok: {p}")),
        ("opencode", format!("opencode ok: {p}")),
    ] {
        let v = test_json(home.path(), bin.path(), head, &[]);
        assert_eq!(v["ok"], true, "{head}: {v}");
        assert_eq!(v["first_line"], want.as_str(), "{head}");
        assert_eq!(v["exit"], 0, "{head}");
        assert!(v["command"].as_str().unwrap().len() > 3);
    }
    let v = test_json(home.path(), bin.path(), "opencode", &[]);
    assert_eq!(v["command"], "opencode run -m zai-coding-plan/glm-5.3 'Answer in five words: is water wet?'");
    let v = test_json(home.path(), bin.path(), "cursor", &[]);
    assert_eq!(v["ok"], false);
    assert_eq!(v["fix"], "sign_in");
    assert_eq!(v["message"], "Cursor Agent says you are not signed in. Sign in to Cursor Agent again.");
}

/// Fake Anthropic API that answers "from the key".
fn fake_anthropic() -> String {
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
            let body = r#"{"content":[{"type":"text","text":"from the key"}]}"#;
            let mut s = stream;
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    format!("http://{addr}/v1/messages")
}

#[test]
fn the_signed_in_tool_comes_first_and_the_key_second() {
    let home = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    fakes(bin.path());
    let url = fake_anthropic();
    let key = [("ANTHROPIC_API_KEY", "sk-test"), ("ROUNDTABLE_ANTHROPIC_URL", url.as_str())];
    // Tool and key: the tool answers.
    let v = test_json(home.path(), bin.path(), "claude", &key);
    assert_eq!((v["via"].as_str(), v["ok"].as_bool()), (Some("Claude Code"), Some(true)));
    // Only the key, on request.
    let (_, out) = run(home.path(), bin.path(), &["test", "claude", "--via", "key", "--json"], &key);
    assert!(out.contains("\"via\":\"API key\"") && out.contains("from the key"), "{out}");
    // A signed-out tool falls back to the key.
    script(bin.path(), "claude", r#"echo "Not logged in. Please run /login" >&2; exit 1"#);
    let v = test_json(home.path(), bin.path(), "claude", &key);
    assert_eq!((v["via"].as_str(), v["first_line"].as_str()), (Some("API key"), Some("from the key")));
    // Neither: skipped, with one line saying what to do.
    let empty = tempfile::tempdir().unwrap();
    let v = test_json(home.path(), empty.path(), "gemini", &[]);
    assert_eq!(v["status"], "skipped");
    assert_eq!(v["message"], "Add a Gemini key or install Gemini CLI.");
    assert_eq!(v["fix"], "add_key");
    let v = test_json(home.path(), empty.path(), "kimi", &[]);
    assert_eq!(v["message"], "Install Kimi to seat Kimi.");
}

#[test]
fn one_failed_head_never_blocks_the_rest() {
    let home = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    fakes(bin.path());
    script(bin.path(), "kimi", "sleep 30");
    let t = std::time::Instant::now();
    let (code, out) = run(home.path(), bin.path(), &["ask", "--events", "--no-history", "--timeout", "3", "--heads", "claude,gemini,kimi,cursor,glm", "-q", "Q?"], &[]);
    assert!(t.elapsed() < std::time::Duration::from_secs(20));
    assert_eq!(code, 0, "{out}");
    let events: Vec<serde_json::Value> = out.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).collect();
    let by = |h: &str| events.iter().find(|e| e["event"] == "head_done" && e["head"] == h).unwrap().clone();
    assert_eq!(by("claude")["status"], "answered");
    assert_eq!(by("claude")["via"], "Claude Code");
    assert_eq!(by("gemini")["status"], "answered");
    assert_eq!(by("kimi")["status"], "timed_out");
    assert_eq!(by("kimi")["fix"], "retry");
    assert_eq!(by("cursor")["fix"], "sign_in");
    assert_eq!(by("glm")["status"], "skipped");
    assert_eq!(by("glm")["message"], "Add a GLM key.");
    let done = events.iter().find(|e| e["event"] == "run_done").unwrap();
    let tr = done["transcript"].as_str().unwrap();
    assert!(tr.contains("## \u{1f535} Claude (Claude Code, your plan)"), "{tr}");
    assert!(tr.contains("_(no answer: Cursor Agent says you are not signed in. Sign in to Cursor Agent again.)_"));
    assert!(!tr.contains('\u{2014}'));
}
