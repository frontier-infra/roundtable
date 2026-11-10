//! Spec 7.4: no file the engine writes may contain a key.
//! Runs the real binary with a marker key, a fake provider and history on,
//! then scans every file under the data folder for the marker.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

const MARKER: &str = "sk-LEAKCHECK-0123456789abcdef";

fn fake_provider() -> String {
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
            let body = r#"{"choices":[{"message":{"content":"An answer."}}]}"#;
            let mut s = stream;
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    format!("http://{addr}/v1/chat/completions")
}

fn scan(dir: &Path, hits: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            scan(&p, hits);
        } else if std::fs::read(&p).unwrap().windows(MARKER.len()).any(|w| w == MARKER.as_bytes()) {
            hits.push(p.display().to_string());
        }
    }
}

fn engine(data: &Path, home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_roundtable-engine"));
    c.env("ROUNDTABLE_DATA_DIR", data)
        .env("ROUNDTABLE_KEYSTORE", "memory")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("ZAI_API_KEY", MARKER);
    c
}

#[test]
fn files_written_by_a_run_hold_no_key() {
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let url = fake_provider();
    let out = engine(data.path(), home.path())
        .args(["ask", "--heads", "glm", "--rounds", "2", "--events", "--source", "app", "-q", "Leak test?"])
        .env("ROUNDTABLE_GLM_URL", &url)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"event\":\"run_done\""), "{stdout}");
    assert!(!stdout.contains(MARKER), "events leaked the key");
    let doctor = engine(data.path(), home.path()).args(["doctor", "--json"]).output().unwrap();
    assert!(!String::from_utf8_lossy(&doctor.stdout).contains(MARKER), "doctor leaked the key");
    let status = engine(data.path(), home.path()).args(["auth", "status", "--json"]).output().unwrap();
    assert!(!String::from_utf8_lossy(&status.stdout).contains(MARKER), "auth status leaked the key");
    assert!(std::fs::read_dir(data.path().join("history")).unwrap().count() >= 2, "history was written");
    let mut hits = Vec::new();
    scan(data.path(), &mut hits);
    assert!(hits.is_empty(), "key found in: {hits:?}");
}
