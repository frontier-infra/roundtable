//! What reaches history: flags never start a run, and setup or test runs stay out.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

fn fake_claude(bin: &Path, home: &Path) {
    let p = bin.join("claude");
    let body = format!("#!/bin/sh\ntouch '{}'\nq=$(cat)\necho \"claude ok\"\n", home.join("called").display());
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn run(home: &Path, bin: &Path, args: &[&str], extra: &[(&str, &str)]) -> (i32, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_roundtable-engine"));
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

fn saved(home: &Path) -> Vec<String> {
    roundtable_engine::history::list(&home.join("data").join("history")).into_iter().map(|e| e.question).collect()
}

const ASK: [&str; 7] = ["ask", "--heads", "claude", "--no-clarify", "--verdict", "none", "--timeout"];

#[test]
fn help_and_version_never_start_a_run() {
    let home = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    fake_claude(bin.path(), home.path());
    for args in [vec!["ask", "--help"], vec!["ask", "-h"], vec!["--help"], vec!["ask", "--heads", "claude", "--help"], vec!["Should we hire?", "--help"]] {
        let (code, out) = run(home.path(), bin.path(), &args, &[]);
        assert_eq!(code, 0, "{args:?}");
        assert!(out.contains("Usage:"), "{args:?}: {out}");
    }
    for args in [vec!["ask", "--version"], vec!["--version"], vec!["ask", "-V"]] {
        let (code, out) = run(home.path(), bin.path(), &args, &[]);
        assert_eq!(code, 0, "{args:?}");
        assert!(out.starts_with("roundtable "), "{args:?}: {out}");
    }
    // A question that is only a flag is no question.
    let (code, _) = run(home.path(), bin.path(), &["ask", "-q", "--help"], &[]);
    assert_eq!(code, 2);
    assert!(!home.path().join("called").exists(), "no AI was asked");
    assert!(saved(home.path()).is_empty());
}

#[test]
fn setup_and_test_runs_stay_out_of_history() {
    let home = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    fake_claude(bin.path(), home.path());
    let ask = |q: &str, more: &[&str], env: &[(&str, &str)]| {
        let mut a: Vec<&str> = ASK.to_vec();
        a.push("20");
        a.extend(more);
        a.extend(["-q", q]);
        run(home.path(), bin.path(), &a, env)
    };
    let (code, out) = ask("Setup check?", &["--source", "setup"], &[]);
    assert_eq!(code, 0, "{out}");
    assert!(home.path().join("called").exists(), "the run happened");
    ask("Test run?", &["--source", "test"], &[]);
    ask("Marked by the harness?", &["--source", "app"], &[("ROUNDTABLE_TEST", "1")]);
    assert!(saved(home.path()).is_empty(), "{:?}", saved(home.path()));
    ask("A real question?", &["--source", "app"], &[]);
    assert_eq!(saved(home.path()), vec!["A real question?"]);
}
