//! "Share with ChatGPT": the engine's Streamable HTTP server on 127.0.0.1 with a fresh
//! token, and a Cloudflare quick tunnel (cloudflared) that gives it a public https URL.
//!
//! Never on by default. Switching it off ends both processes (by the handles we started,
//! never by name) and the token with them; the next start makes a new token and URL.
//! The app quits them on exit too. Every request the server sees is logged and shown.

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

const KEEP_LOG: usize = 200;

struct Running {
    engine: Child,
    tunnel: Child,
    base: String,
    token: String,
    started: u64,
}

#[derive(Default)]
pub struct Share {
    running: Mutex<Option<Running>>,
    log: Arc<Mutex<VecDeque<Value>>>,
    /// Why the last share ended on its own, for the UI.
    ended: Mutex<Option<String>>,
}

/// What to tell the person when cloudflared is missing.
pub fn install_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "Install cloudflared first: in Terminal, run brew install cloudflared (or download it from developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads). Then switch this on again."
    } else if cfg!(windows) {
        "Install cloudflared first: in PowerShell, run winget install --id Cloudflare.cloudflared. Then switch this on again."
    } else {
        "Install cloudflared first (developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads). Then switch this on again."
    }
}

/// Find `cloudflared`: PATH, the usual install folders, then the login shell (a Mac app has a short PATH).
pub fn find_cloudflared() -> Option<PathBuf> {
    if let Some(p) = roundtable_engine::council::find_exe("cloudflared") {
        return Some(p);
    }
    let mut known = vec![PathBuf::from("/opt/homebrew/bin/cloudflared"), PathBuf::from("/usr/local/bin/cloudflared"), PathBuf::from("/usr/bin/cloudflared")];
    if cfg!(windows) {
        if let Some(pf) = std::env::var_os("ProgramFiles(x86)") {
            known.push(PathBuf::from(pf).join("cloudflared/cloudflared.exe"));
        }
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            known.push(PathBuf::from(pf).join("cloudflared/cloudflared.exe"));
        }
    }
    if let Some(p) = known.into_iter().find(|p| p.is_file()) {
        return Some(p);
    }
    #[cfg(target_os = "macos")]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        if let Ok(o) = Command::new(shell).args(["-lc", "command -v cloudflared"]).output() {
            let p = PathBuf::from(String::from_utf8_lossy(&o.stdout).trim());
            if p.is_absolute() && p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// The quick tunnel's URL in one line of cloudflared's log.
pub fn tunnel_url(line: &str) -> Option<String> {
    let i = line.find("https://")?;
    let rest = &line[i..];
    let end = rest.find(|c: char| c.is_whitespace() || c == '|' || c == '"').unwrap_or(rest.len());
    let url = &rest[..end];
    let host = url.trim_start_matches("https://");
    (host.ends_with(".trycloudflare.com") && host.len() > ".trycloudflare.com".len() && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')).then(|| url.to_string())
}

/// The engine's `--events` line that says where it listens.
pub fn listening_port(line: &str) -> Option<u16> {
    let v: Value = serde_json::from_str(line).ok()?;
    (v["type"] == "listening").then(|| v["port"].as_u64()).flatten().map(|p| p as u16)
}

fn push(log: &Mutex<VecDeque<Value>>, v: Value) {
    let mut l = log.lock().unwrap();
    l.push_back(v);
    while l.len() > KEEP_LOG {
        l.pop_front();
    }
}

impl Share {
    fn status_of(&self, r: Option<&Running>) -> Value {
        let log: Vec<Value> = self.log.lock().unwrap().iter().cloned().collect();
        match r {
            Some(r) => json!({
                "running": true,
                "url": format!("{}/mcp/{}", r.base, r.token),
                "server_url": format!("{}/mcp", r.base),
                "token": r.token,
                "started": r.started,
                "log": log,
            }),
            None => json!({"running": false, "log": log, "ended": *self.ended.lock().unwrap(), "cloudflared": find_cloudflared().is_some(), "install": install_hint()}),
        }
    }

    /// Status. A process that ended on its own ends the share.
    pub fn status(&self) -> Value {
        let mut g = self.running.lock().unwrap();
        if let Some(r) = g.as_mut() {
            let gone = match (r.engine.try_wait(), r.tunnel.try_wait()) {
                (Ok(Some(_)), _) => Some("The Roundtable server stopped, so sharing stopped."),
                (_, Ok(Some(_))) => Some("The tunnel closed, so sharing stopped. Switch it on again for a new link."),
                _ => None,
            };
            if let Some(why) = gone {
                let mut r = g.take().unwrap();
                let _ = r.engine.kill();
                let _ = r.tunnel.kill();
                let _ = r.engine.wait();
                let _ = r.tunnel.wait();
                *self.ended.lock().unwrap() = Some(why.into());
                push(&self.log, json!({"type": "stopped", "ts": roundtable_engine::history::now(), "note": why}));
            }
        }
        self.status_of(g.as_ref())
    }

    pub fn start(&self, engine: &Path) -> Result<Value, String> {
        let mut g = self.running.lock().unwrap();
        if g.is_some() {
            drop(g);
            return Ok(self.status());
        }
        let cloudflared = find_cloudflared().ok_or_else(|| install_hint().to_string())?;
        let token = roundtable_engine::mcp_http::new_token();
        // The server: this computer only, a free port, the token through the environment (not argv).
        let mut c = Command::new(engine);
        c.args(["mcp", "serve", "--http", "--host", "127.0.0.1", "--port", "0", "--events"])
            .env("ROUNDTABLE_MCP_TOKEN", &token)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::engine_install::no_window(&mut c);
        let mut eng = c.spawn().map_err(|e| format!("Could not start the Roundtable server: {e}"))?;
        let (port_tx, port_rx) = mpsc::channel();
        let out = eng.stdout.take().unwrap();
        let log = self.log.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(p) = listening_port(&line) {
                    let _ = port_tx.send(p);
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    push(&log, v);
                }
            }
        });
        let port = match port_rx.recv_timeout(Duration::from_secs(15)) {
            Ok(p) => p,
            Err(_) => {
                let _ = eng.kill();
                let _ = eng.wait();
                return Err("The Roundtable server did not start.".into());
            }
        };
        // The tunnel. An empty config file keeps a named tunnel's config.yml out of it.
        let empty = std::env::temp_dir().join("roundtable-quick-tunnel.yml");
        let _ = std::fs::write(&empty, "");
        let mut t = Command::new(&cloudflared);
        t.args(["tunnel", "--no-autoupdate", "--config"])
            .arg(&empty)
            .args(["--url", &format!("http://127.0.0.1:{port}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        crate::engine_install::no_window(&mut t);
        let mut tunnel = match t.spawn() {
            Ok(t) => t,
            Err(e) => {
                let _ = eng.kill();
                let _ = eng.wait();
                return Err(format!("Could not start cloudflared: {e}"));
            }
        };
        let (url_tx, url_rx) = mpsc::channel();
        let err = tunnel.stderr.take().unwrap();
        std::thread::spawn(move || {
            let mut sent = false;
            // Keep reading so cloudflared never blocks on a full pipe.
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if !sent {
                    if let Some(u) = tunnel_url(&line) {
                        let _ = url_tx.send(u);
                        sent = true;
                    }
                }
            }
        });
        let base = match url_rx.recv_timeout(Duration::from_secs(45)) {
            Ok(u) => u,
            Err(_) => {
                let _ = tunnel.kill();
                let _ = eng.kill();
                let _ = tunnel.wait();
                let _ = eng.wait();
                return Err("Cloudflare did not give a link. Check the internet connection and try again.".into());
            }
        };
        let started = roundtable_engine::history::now();
        *self.ended.lock().unwrap() = None;
        push(&self.log, json!({"type": "started", "ts": started, "note": format!("Sharing at {base}")}));
        *g = Some(Running { engine: eng, tunnel, base, token, started });
        drop(g);
        Ok(self.status())
    }

    /// Stop both processes. Safe to call when nothing runs.
    pub fn stop(&self) -> Value {
        if let Some(mut r) = self.running.lock().unwrap().take() {
            let _ = r.tunnel.kill();
            let _ = r.engine.kill();
            let _ = r.tunnel.wait();
            let _ = r.engine.wait();
            push(&self.log, json!({"type": "stopped", "ts": roundtable_engine::history::now(), "note": "Sharing stopped. That link and token no longer work."}));
        }
        self.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_quick_tunnel_url() {
        let l = "2026-11-10T19:01:02Z INF |  https://brave-lion-river-sky.trycloudflare.com                                     |";
        assert_eq!(tunnel_url(l).as_deref(), Some("https://brave-lion-river-sky.trycloudflare.com"));
        assert_eq!(tunnel_url("INF Requesting new quick Tunnel on trycloudflare.com..."), None);
        assert_eq!(tunnel_url("see https://developers.cloudflare.com/x"), None);
        assert_eq!(tunnel_url("https://.trycloudflare.com"), None);
    }

    #[test]
    fn reads_the_listening_event() {
        assert_eq!(listening_port(r#"{"type":"listening","url":"http://127.0.0.1:5123/mcp","port":5123}"#), Some(5123));
        assert_eq!(listening_port(r#"{"type":"request","port":1}"#), None);
        assert_eq!(listening_port("not json"), None);
    }

    #[test]
    fn off_by_default_and_stop_is_safe() {
        let s = Share::default();
        let st = s.status();
        assert_eq!(st["running"], false);
        assert!(st["install"].as_str().unwrap().contains("cloudflared"));
        assert_eq!(s.stop()["running"], false);
    }

    /// A real share: the engine and a Cloudflare quick tunnel. Needs cloudflared and the internet.
    ///   ROUNDTABLE_TEST_ENGINE=<engine> cargo test -p roundtable-desktop share -- --ignored --nocapture
    #[test]
    #[ignore = "starts a real Cloudflare quick tunnel"]
    fn a_real_share_starts_and_stops() {
        let engine = PathBuf::from(std::env::var("ROUNDTABLE_TEST_ENGINE").expect("set ROUNDTABLE_TEST_ENGINE"));
        let s = Share::default();
        let st = s.start(&engine).unwrap();
        assert_eq!(st["running"], true);
        assert!(st["url"].as_str().unwrap().contains(".trycloudflare.com/mcp/"));
        println!("server_url {}", st["server_url"]);
        let st = s.stop();
        assert_eq!(st["running"], false);
    }
}
