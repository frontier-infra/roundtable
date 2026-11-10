//! AI command-line tools a person already has installed and signed in to
//! (Claude Code, Codex, Gemini CLI, Grok and others).
//!
//! Detection sends no prompt and never reads a secret. It runs each tool's own
//! `--version` and status command, or checks that the tool's credential file
//! exists (by path only, never opened).
//!
//! Heads run through a tool in its headless print mode, with the flags the bash
//! engine (`lib/core.sh`) uses for codex and grok.

use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How to tell whether a tool is signed in, without sending a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthCheck {
    /// `claude auth status --json` prints `{"loggedIn": true, "authMethod": ...}`.
    ClaudeStatus,
    /// `codex login status` prints "Logged in using ChatGPT". Falls back to
    /// `auth.json` in `CODEX_HOME`, as `rt_codex_logged_in` in `lib/config.sh` does.
    CodexStatus,
    /// `cursor-agent status` prints "Logged in as ..." or "Not logged in".
    CursorStatus,
    /// `opencode auth list` names each saved account (never the secret).
    OpencodeList,
    /// `~/.qwen/oauth_creds.json` (Qwen sign-in), else the auth type chosen in its settings.
    Qwen,
    /// Any of these home-relative paths exists (a folder must not be empty). Never opened.
    Files(&'static [&'static str]),
}

/// How the prompt reaches the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptVia {
    Arg,
    Stdin,
}

#[derive(Debug)]
pub struct Cli {
    /// Tool id: claude, codex, gemini, ...
    pub id: &'static str,
    /// The head it seats at the table.
    pub head: &'static str,
    /// Name people know it by.
    pub name: &'static str,
    /// Executable names to look for, first wins.
    pub exes: &'static [&'static str],
    /// One plain sentence: whose account pays.
    pub plan: &'static str,
    /// Arguments that start the tool's own sign-in.
    pub login: &'static [&'static str],
    pub auth: AuthCheck,
}

/// Every tool Roundtable knows. Order is the order on screen.
pub const CLIS: &[Cli] = &[
    Cli { id: "claude", head: "claude", name: "Claude Code", exes: &["claude"], plan: "Uses your Claude plan.", login: &["auth", "login"], auth: AuthCheck::ClaudeStatus },
    Cli { id: "codex", head: "codex", name: "Codex", exes: &["codex"], plan: "Uses your ChatGPT plan.", login: &["login"], auth: AuthCheck::CodexStatus },
    Cli { id: "gemini", head: "gemini", name: "Gemini CLI", exes: &["gemini"], plan: "Uses your Google account.", login: &[], auth: AuthCheck::Files(&[".gemini/oauth_creds.json"]) },
    Cli { id: "grok", head: "grok", name: "Grok", exes: &["grok"], plan: "Uses your Grok plan.", login: &["login"], auth: AuthCheck::Files(&[".grok/auth.json", ".grok/api-key"]) },
    Cli { id: "qwen", head: "qwen", name: "Qwen Code", exes: &["qwen"], plan: "Uses your Qwen account.", login: &[], auth: AuthCheck::Qwen },
    Cli { id: "kimi", head: "kimi", name: "Kimi", exes: &["kimi"], plan: "Uses your Kimi account.", login: &["login"], auth: AuthCheck::Files(&[".kimi-code/credentials", ".kimi/credentials"]) },
    Cli { id: "opencode", head: "opencode", name: "OpenCode", exes: &["opencode"], plan: "Uses the accounts you set up in OpenCode.", login: &["auth", "login"], auth: AuthCheck::OpencodeList },
    Cli { id: "cursor", head: "cursor", name: "Cursor Agent", exes: &["cursor-agent"], plan: "Uses your Cursor plan.", login: &["login"], auth: AuthCheck::CursorStatus },
];

pub fn find(id: &str) -> Option<&'static Cli> {
    CLIS.iter().find(|c| c.id == id)
}

pub fn for_head(head: &str) -> Option<&'static Cli> {
    CLIS.iter().find(|c| c.head == head)
}

// ---- Where tools live ----

/// Folders to search: PATH first, then the usual per-user and system install folders.
/// An app started from the Dock or Start menu gets a short PATH, so the list matters.
pub fn search_dirs() -> Vec<PathBuf> {
    // Tests and the parity check: look on PATH only, so tools installed on the
    // machine running the test never answer for a fake one.
    // In-process tests: exactly these folders (empty: none).
    if let Some(dirs) = std::env::var_os("ROUNDTABLE_TOOL_PATH") {
        return std::env::split_paths(&dirs).filter(|d| !d.as_os_str().is_empty()).collect();
    }
    if std::env::var_os("ROUNDTABLE_PATH_ONLY").is_some() {
        return std::env::var_os("PATH").map(|p| std::env::split_paths(&p).filter(|d| !d.as_os_str().is_empty()).collect()).unwrap_or_default();
    }
    let mut path: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    path.extend(login_shell_path().iter().cloned());
    search_dirs_for(std::env::join_paths(path).ok(), &crate::paths::home())
}

/// The PATH the person's own terminal has (Mac and Linux), read once.
/// The Dock starts apps with a short PATH, so without this a tool the terminal
/// finds first (Homebrew, say) could lose to an older copy elsewhere.
fn login_shell_path() -> &'static Vec<PathBuf> {
    static P: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        if cfg!(windows) || std::env::var_os("ROUNDTABLE_NO_SHELL_PATH").is_some() {
            return Vec::new();
        }
        let Some(shell) = std::env::var_os("SHELL").filter(|s| !s.is_empty()) else { return Vec::new() };
        let mut c = Command::new(shell);
        c.args(["-ilc", "printf '__RT__%s__RT__' \"$PATH\""]);
        match run_with_timeout(c, None, Duration::from_secs(5)) {
            Ok(Some(r)) => r
                .stdout
                .split("__RT__")
                .nth(1)
                .map(|p| std::env::split_paths(p).filter(|d| d.is_absolute()).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    })
}

pub fn search_dirs_for(path: Option<OsString>, home: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path.map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for rel in [
        ".local/bin",
        ".claude/local",
        ".bun/bin",
        ".npm-global/bin",
        ".npm/bin",
        ".volta/bin",
        ".cargo/bin",
        ".grok/bin",
        ".opencode/bin",
        ".kimi-code/bin",
        ".yarn/bin",
        "Library/pnpm",
        ".local/share/pnpm",
    ] {
        dirs.push(home.join(rel));
    }
    // Node version managers: newest version first.
    for (base, tail) in [(".nvm/versions/node", "bin"), (".local/share/fnm/node-versions", "installation/bin"), ("Library/Application Support/fnm/node-versions", "installation/bin")] {
        if let Ok(rd) = std::fs::read_dir(home.join(base)) {
            let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path().join(tail)).collect();
            v.sort();
            v.reverse();
            dirs.extend(v);
        }
    }
    if cfg!(target_os = "macos") {
        dirs.push("/opt/homebrew/bin".into());
        dirs.push("/usr/local/bin".into());
    }
    if cfg!(target_os = "linux") {
        dirs.push("/usr/local/bin".into());
    }
    if cfg!(windows) {
        if let Some(roaming) = dirs::data_dir() {
            dirs.push(roaming.join("npm"));
        }
        if let Some(local) = dirs::data_local_dir() {
            dirs.push(local.join("pnpm"));
            dirs.push(local.join("Microsoft").join("WinGet").join("Links"));
            dirs.push(local.join("Programs").join("claude"));
            dirs.push(local.join("cursor-agent"));
        }
        dirs.push(home.join("scoop").join("shims"));
        dirs.push(home.join(".bun").join("bin"));
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
    dirs
}

/// Find an executable in these folders.
pub fn find_in(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat", ""] } else { &[""] };
    for d in dirs {
        for e in exts {
            let p = d.join(format!("{name}{e}"));
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Look for an executable on PATH and in the usual per-user folders.
pub fn find_exe(name: &str) -> Option<PathBuf> {
    find_in(name, &search_dirs())
}

pub fn find_cli(cli: &Cli) -> Option<PathBuf> {
    let dirs = search_dirs();
    cli.exes.iter().find_map(|n| find_in(n, &dirs))
}

/// PATH for a tool's process: its own folder first, then every search folder,
/// so a `#!/usr/bin/env node` script finds node even when the app has a short PATH.
pub fn child_path(exe: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = exe.parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    dirs.extend(search_dirs());
    if cfg!(unix) {
        for d in ["/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
            dirs.push(d.into());
        }
    }
    let sep = if cfg!(windows) { ';' } else { ':' };
    std::env::join_paths(dirs.iter().filter(|d| {
        let s = d.to_string_lossy();
        !s.contains(sep) && !s.contains('"')
    }))
    .unwrap_or_default()
}

/// A command for a tool, with the full PATH and no console window.
pub fn tool_command(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    c.env("PATH", child_path(exe));
    c.env("NO_COLOR", "1");
    no_window(&mut c);
    c
}

pub fn no_window(_c: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _c.creation_flags(0x0800_0000);
    }
}

// ---- Running with a time limit ----

/// What a finished tool printed.
#[derive(Debug, Clone, PartialEq)]
pub struct Ran {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Run a command with no terminal. Feeds `input` on stdin (or none), captures both streams.
/// Returns `Ok(None)` on timeout, after ending the whole process group.
pub fn run_with_timeout(mut cmd: Command, input: Option<&str>, timeout: Duration) -> Result<Option<Ran>, String> {
    cmd.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    if let (Some(text), Some(mut stdin)) = (input.map(str::to_owned), child.stdin.take()) {
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let pipe = |r: Option<Box<dyn std::io::Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
        });
        rx
    };
    let out = pipe(child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>));
    let err = pipe(child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>));
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                // A helper the tool left running may hold the pipe; do not wait on it forever.
                let grace = Duration::from_secs(3);
                return Ok(Some(Ran {
                    code: st.code().unwrap_or(-1),
                    stdout: out.recv_timeout(grace).unwrap_or_default(),
                    stderr: err.recv_timeout(grace).unwrap_or_default(),
                }));
            }
            Ok(None) if start.elapsed() > timeout => {
                kill_tree(&mut child);
                return Ok(None);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // The child leads its own process group, so this ends anything it started too.
        let _ = Command::new("kill").args(["-9", &format!("-{}", child.id())]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    #[cfg(windows)]
    {
        let mut c = Command::new("taskkill");
        c.args(["/T", "/F", "/PID", &child.id().to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
        no_window(&mut c);
        let _ = c.status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

// ---- Detection ----

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Detection {
    pub id: String,
    pub head: String,
    pub name: String,
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// `None` when the tool gives no way to tell.
    pub signed_in: Option<bool>,
    /// One plain sentence: whose account pays.
    pub note: String,
    /// How Roundtable calls it, for the curious.
    pub how: String,
    /// The tool's own sign-in command.
    pub login: String,
}

/// Pull a version number out of a `--version` line: "2.1.3 (Claude Code)" gives "2.1.3".
pub fn parse_version(out: &str) -> Option<String> {
    let line = out.lines().map(str::trim).find(|l| !l.is_empty())?;
    line.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
        .map(|t| t.trim_start_matches(['v', 'V']))
        .find(|t| t.contains('.') && t.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .or_else(|| Some(line.chars().take(40).collect()))
}

fn path_present(p: &Path) -> bool {
    match std::fs::metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p).is_ok_and(|mut d| d.next().is_some()),
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Signed in, and a better plan sentence when the status says how.
fn check_auth(cli: &Cli, exe: &Path, home: &Path) -> (Option<bool>, Option<&'static str>) {
    let status = |args: &[&str]| -> Option<Ran> {
        let mut c = tool_command(exe);
        c.args(args).current_dir(home);
        run_with_timeout(c, None, Duration::from_secs(20)).ok().flatten()
    };
    match cli.auth {
        AuthCheck::ClaudeStatus => match status(&["auth", "status", "--json"]) {
            Some(r) => match serde_json::from_str::<serde_json::Value>(r.stdout.trim()) {
                Ok(v) => {
                    let ok = v["loggedIn"].as_bool();
                    let method = v["authMethod"].as_str().unwrap_or("");
                    let note = if method.contains("api_key") || method.contains("apiKey") { Some("Uses your Claude Code sign-in.") } else { None };
                    (ok, note)
                }
                // Older versions: no status command. Linux and Windows keep a credential file.
                Err(_) => (Some(path_present(&home.join(".claude/.credentials.json"))).filter(|b| *b), None),
            },
            None => (None, None),
        },
        AuthCheck::CodexStatus => {
            let codex_home = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex"));
            match status(&["login", "status"]) {
                Some(r) => {
                    let t = format!("{}\n{}", r.stdout, r.stderr).to_ascii_lowercase();
                    if t.contains("not logged in") {
                        (Some(false), None)
                    } else if t.contains("logged in") {
                        (Some(true), if t.contains("api key") { Some("Uses your OpenAI account.") } else { None })
                    } else {
                        (Some(path_present(&codex_home.join("auth.json"))), None)
                    }
                }
                None => (Some(path_present(&codex_home.join("auth.json"))), None),
            }
        }
        AuthCheck::CursorStatus => match status(&["status"]) {
            Some(r) => {
                let t = format!("{}\n{}", r.stdout, r.stderr).to_ascii_lowercase();
                if t.contains("not logged in") || t.contains("not authenticated") {
                    (Some(false), None)
                } else if t.contains("logged in") {
                    (Some(true), None)
                } else {
                    (None, None)
                }
            }
            None => (None, None),
        },
        AuthCheck::OpencodeList => match status(&["auth", "list"]) {
            Some(r) if r.code == 0 => {
                let t = r.stdout.to_ascii_lowercase();
                (Some(t.lines().any(|l| l.contains("stored") || l.contains("oauth") || l.contains("api key"))), None)
            }
            _ => (Some(path_present(&home.join(".local/share/opencode/auth.json"))), None),
        },
        AuthCheck::Qwen => {
            if path_present(&home.join(".qwen/oauth_creds.json")) {
                (Some(true), None)
            } else {
                // Only the chosen auth type is read, never a key. A key-based setup cannot be
                // checked without a request, so it is "unknown", not "signed out".
                let chosen = std::fs::read_to_string(home.join(".qwen/settings.json"))
                    .ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .and_then(|v| v.pointer("/security/auth/selectedType").and_then(|x| x.as_str()).map(str::to_string));
                match chosen {
                    Some(t) if !t.is_empty() && t != "qwen-oauth" => (None, Some("Uses the account set up in Qwen Code.")),
                    _ => (Some(false), None),
                }
            }
        }
        AuthCheck::Files(paths) => (Some(paths.iter().any(|p| path_present(&home.join(p)))), None),
    }
}

/// Detect one tool. No prompt is sent.
pub fn detect(cli: &Cli) -> Detection {
    detect_at(cli, find_cli(cli), &crate::paths::home())
}

pub fn detect_at(cli: &Cli, exe: Option<PathBuf>, home: &Path) -> Detection {
    let login = std::iter::once(cli.exes[0]).chain(cli.login.iter().copied()).collect::<Vec<_>>().join(" ");
    let how = how_line(cli);
    let mut d = Detection {
        id: cli.id.into(),
        head: cli.head.into(),
        name: cli.name.into(),
        found: false,
        path: None,
        version: None,
        signed_in: None,
        note: cli.plan.into(),
        how,
        login,
    };
    let Some(exe) = exe else { return d };
    d.found = true;
    d.path = Some(exe.to_string_lossy().into_owned());
    let mut c = tool_command(&exe);
    c.arg("--version").current_dir(home);
    if let Ok(Some(r)) = run_with_timeout(c, None, Duration::from_secs(20)) {
        d.version = parse_version(&r.stdout).or_else(|| parse_version(&r.stderr));
    }
    let (signed_in, note) = check_auth(cli, &exe, home);
    d.signed_in = signed_in;
    if let Some(n) = note {
        d.note = n.into();
    }
    d
}

/// Detect every tool at once (each in its own thread; the slowest sets the time).
pub fn detect_all() -> Vec<Detection> {
    std::thread::scope(|s| {
        let hs: Vec<_> = CLIS.iter().map(|c| s.spawn(move || detect(c))).collect();
        hs.into_iter().zip(CLIS).map(|(h, c)| h.join().unwrap_or_else(|_| detect_at(c, None, Path::new(".")))).collect()
    })
}

// ---- Running a head through a tool ----

/// The short prompt the Test button and the setup check send.
pub const TEST_PROMPT: &str = "Answer in five words: is water wet?";

/// Arguments for one headless answer. `out_file` is where codex writes its last message.
/// Every flag here was read from the tool's own `--help` (see desktop/README.md, "AI apps").
pub fn headless_args(cli: &Cli, prompt: &str, research: bool, out_file: &Path) -> (Vec<OsString>, PromptVia) {
    headless_args_with(cli, prompt, research, out_file, None)
}

/// Same, with a model the tool should use instead of its own default.
pub fn headless_args_with(cli: &Cli, prompt: &str, research: bool, out_file: &Path, model: Option<&str>) -> (Vec<OsString>, PromptVia) {
    let mut a: Vec<OsString> = Vec::new();
    let mut push = |s: &str| a.push(s.into());
    let via = match cli.id {
        "claude" => {
            for s in ["-p", "--output-format", "text", "--strict-mcp-config", "--no-session-persistence"] {
                push(s);
            }
            if !research {
                push("--disallowedTools");
                push("WebSearch,WebFetch");
            }
            PromptVia::Stdin
        }
        // Same flags as run_codex in lib/core.sh.
        "codex" => {
            for s in ["exec", "--skip-git-repo-check", "-s", "read-only"] {
                push(s);
            }
            if !research {
                push("-c");
                push("tools.web_search=false");
            }
            push("-o");
            a.push(out_file.as_os_str().to_owned());
            if cfg!(windows) {
                a.push("-".into());
                return (a, PromptVia::Stdin);
            }
            a.push(prompt.into());
            PromptVia::Arg
        }
        // `grok -p` is its single-turn mode. The tool picks its own default model: a
        // fixed id breaks as soon as the plan's model list changes (rc4 sent one that
        // no longer exists). ROUNDTABLE_GROK_CLI_MODEL still pins one.
        "grok" => {
            if cfg!(windows) {
                push("--prompt-file");
                a.push(out_file.with_extension("prompt").into_os_string());
            } else {
                push("-p");
                a.push(prompt.into());
            }
            let mut push = |s: &str| a.push(s.into());
            let pinned = std::env::var("ROUNDTABLE_GROK_CLI_MODEL").ok().filter(|m| !m.trim().is_empty());
            if let Some(m) = model.map(str::to_string).or(pinned) {
                push("-m");
                push(&m);
            }
            push("--max-turns");
            push(if research { "8" } else { "6" });
            // One councilor answers; no helper agents.
            push("--no-subagents");
            if !research {
                push("--disable-web-search");
            }
            PromptVia::Arg
        }
        // Piped input runs these headless. Gemini refuses an untrusted folder
        // without --skip-trust, and every run uses a fresh empty folder.
        "gemini" => {
            for s in ["--skip-trust", "--output-format", "text"] {
                push(s);
            }
            PromptVia::Stdin
        }
        "qwen" => {
            push("--output-format");
            push("text");
            PromptVia::Stdin
        }
        // kimi 2.5: `-p` runs one prompt and prints it. It has no --quiet, and
        // refuses --plan together with --prompt.
        "kimi" => {
            push("-p");
            a.push(prompt.into());
            let mut push = |s: &str| a.push(s.into());
            push("--output-format");
            push("text");
            PromptVia::Arg
        }
        "opencode" => {
            push("run");
            if let Some(m) = model {
                push("-m");
                push(m);
            }
            a.push(prompt.into());
            PromptVia::Arg
        }
        "cursor" => {
            for s in ["-p", "--output-format", "text", "--trust", "--mode", "ask"] {
                push(s);
            }
            a.push(prompt.into());
            PromptVia::Arg
        }
        _ => PromptVia::Stdin,
    };
    (a, via)
}

/// Extra environment for a tool's headless run.
///
/// Gemini CLI: a sign-in saved by an older version keeps the choice in the old
/// `selectedAuthType` field, which headless mode no longer reads, so it stops with
/// "Invalid auth method selected". GOOGLE_GENAI_USE_GCA picks the Google sign-in.
/// Only the auth type name is read from settings, never a credential.
pub fn headless_env(cli: &Cli, home: &Path) -> Vec<(&'static str, String)> {
    let mut env = Vec::new();
    if cli.id == "gemini" && std::env::var_os("GEMINI_API_KEY").is_none() && std::env::var_os("GOOGLE_GENAI_USE_GCA").is_none() {
        let v = std::fs::read_to_string(home.join(".gemini/settings.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        let current = v.as_ref().and_then(|v| v.pointer("/security/auth/selectedType")).and_then(|x| x.as_str()).unwrap_or("");
        let legacy = v.as_ref().and_then(|v| v.get("selectedAuthType")).and_then(|x| x.as_str()).unwrap_or("");
        if current.is_empty() && (legacy == "oauth-personal" || (legacy.is_empty() && path_present(&home.join(".gemini/oauth_creds.json")))) {
            env.push(("GOOGLE_GENAI_USE_GCA", "true".to_string()));
        }
    }
    env
}

/// The command line shown to people, with the question left out.
pub fn how_line(cli: &Cli) -> String {
    command_line(cli, "<question>", false, None)
}

/// One printable command line. The prompt is quoted; a piped prompt shows as `< question`.
pub fn command_line(cli: &Cli, prompt: &str, research: bool, model: Option<&str>) -> String {
    let (args, via) = headless_args_with(cli, prompt, research, Path::new("<answer file>"), model);
    let mut s = cli.exes[0].to_string();
    for a in args {
        let a = a.to_string_lossy().into_owned();
        s.push(' ');
        if a.contains(' ') {
            s.push_str(&sh_quote(&a));
        } else {
            s.push_str(&a);
        }
    }
    if via == PromptVia::Stdin {
        s.push_str(" < question");
    }
    s
}

/// What to do about a failed head, in one word the app turns into a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fix {
    /// Sign in to the tool again.
    SignIn,
    /// Add an API key instead.
    AddKey,
    /// Install the tool.
    Install,
    /// Try again later.
    Retry,
}

/// A failed headless run, in plain words.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub fix: Fix,
    pub message: String,
}

/// What came back from one headless run.
#[derive(Debug, Clone, PartialEq)]
pub enum CliResult {
    Answered(String),
    Missing,
    Timeout,
    Failed(Failure),
}

/// The record of one run, for the Test button and the report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trial {
    pub tool: String,
    pub command: String,
    /// None on a time-out.
    pub exit: Option<i32>,
    pub secs: f64,
    /// Last lines of what the tool printed on stderr, colour codes removed.
    pub stderr_tail: String,
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                for d in it.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn tail(s: &str, n: usize) -> String {
    let t = s.trim();
    let start = t.char_indices().rev().nth(n.saturating_sub(1)).map(|(i, _)| i).unwrap_or(0);
    t[start..].to_string()
}

/// Turn a tool's exit code and output into one plain sentence and a fix.
pub fn explain(cli: &Cli, code: i32, stdout: &str, stderr: &str) -> Failure {
    let both = format!("{stdout}\n{stderr}");
    let t = both.to_ascii_lowercase();
    let any = |ws: &[&str]| ws.iter().any(|w| t.contains(w));
    let name = cli.name;
    if any(&["no longer supported for gemini code assist", "ineligibletiererror", "unsupported_client"]) {
        return Failure { fix: Fix::AddKey, message: "Google no longer lets Gemini CLI answer on a personal Google sign-in. Add a Gemini key instead.".into() };
    }
    if any(&[
        "authentication required",
        "not logged in",
        "not signed in",
        "please log in",
        "please login",
        "login required",
        "agent login",
        "authorization grant is invalid",
        "no credential configured",
        "no credentials",
        "credentials not found",
        "invalid_grant",
        "not authenticated",
        "unauthenticated",
        "unauthorized",
        "incorrect api key",
        "invalid api key",
        "invalid auth method",
        "api error: 401",
        "status 401",
        "http 401",
        " 401 ",
    ]) {
        return Failure { fix: Fix::SignIn, message: format!("{name} says you are not signed in. Sign in to {name} again.") };
    }
    if any(&["usage limit", "rate limit", "rate-limit", "too many requests", " 429", "quota", "out of credits", "insufficient balance"]) {
        return Failure { fix: Fix::Retry, message: format!("{name} hit its usage limit. Try again later.") };
    }
    if any(&["unknown model", "model not found", "invalid model", "couldn't set model"]) {
        return Failure { fix: Fix::Retry, message: format!("{name} does not offer the model it was asked to use.") };
    }
    let clean = strip_ansi(stderr);
    let lines: Vec<&str> = clean.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let noise = |l: &str| {
        let t = l.to_ascii_lowercase();
        l.starts_with("at ") || t.starts_with("warning") || t.starts_with("see log") || t.starts_with("for more details")
    };
    // An "error" line says the most; else the last line that is not noise.
    let line = lines
        .iter()
        .rev()
        .find(|l| l.to_ascii_lowercase().contains("error") && !noise(l))
        .or_else(|| lines.iter().rev().find(|l| !noise(l)))
        .map(|l| l.chars().take(140).collect::<String>())
        .unwrap_or_default();
    let message = if line.is_empty() {
        format!("{name} stopped without an answer (exit {code}).")
    } else {
        format!("{name} stopped with an error: {line}")
    };
    Failure { fix: Fix::Retry, message }
}

/// Ask one tool one question, headless, in an empty folder, with a time limit.
pub fn ask(cli: &Cli, prompt: &str, research: bool, timeout: Duration) -> CliResult {
    let Some(exe) = find_cli(cli) else { return CliResult::Missing };
    ask_with(cli, &exe, prompt, research, timeout)
}

pub fn ask_with(cli: &Cli, exe: &Path, prompt: &str, research: bool, timeout: Duration) -> CliResult {
    ask_detailed(cli, exe, prompt, research, timeout).0
}

/// Ask, and also say exactly what ran. A tool that fails on its own default model
/// (OpenCode pointed at a provider that is down, say) is asked once more with a
/// model from an account it is signed in to.
pub fn ask_detailed(cli: &Cli, exe: &Path, prompt: &str, research: bool, timeout: Duration) -> (CliResult, Trial) {
    let start = Instant::now();
    let pinned = if cli.id == "opencode" { std::env::var("ROUNDTABLE_OPENCODE_MODEL").ok().filter(|m| !m.trim().is_empty()) } else { None };
    let (r, mut trial) = run_once(cli, exe, prompt, research, timeout, pinned.as_deref());
    if let CliResult::Failed(_) = &r {
        if pinned.is_none() {
            let left = timeout.saturating_sub(start.elapsed());
            if let Some(m) = fallback_model(cli, exe).filter(|_| left > Duration::from_secs(5)) {
                let (r2, t2) = run_once(cli, exe, prompt, research, left, Some(&m));
                if matches!(r2, CliResult::Answered(_)) {
                    let mut t2 = t2;
                    t2.secs = round1(start.elapsed().as_secs_f64());
                    return (r2, t2);
                }
            }
        }
    }
    trial.secs = round1(start.elapsed().as_secs_f64());
    (r, trial)
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn run_once(cli: &Cli, exe: &Path, prompt: &str, research: bool, timeout: Duration, model: Option<&str>) -> (CliResult, Trial) {
    let dir = std::env::temp_dir().join(format!("roundtable-{}-{}-{}", cli.id, std::process::id(), nanos()));
    let work = dir.join("work");
    let _ = std::fs::create_dir_all(&work);
    let last = dir.join("last.md");
    let (args, via) = headless_args_with(cli, prompt, research, &last, model);
    if cli.id == "grok" && cfg!(windows) {
        let _ = std::fs::write(last.with_extension("prompt"), prompt);
    }
    let mut c = tool_command(exe);
    c.args(&args).current_dir(&work);
    for (k, v) in headless_env(cli, &crate::paths::home()) {
        c.env(k, v);
    }
    let started = Instant::now();
    let r = run_with_timeout(c, if via == PromptVia::Stdin { Some(prompt) } else { None }, timeout);
    let secs = round1(started.elapsed().as_secs_f64());
    let file_text = std::fs::read_to_string(&last).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let short: String = if prompt.chars().count() > 60 { "<question>".into() } else { prompt.into() };
    let mut trial = Trial { tool: cli.name.into(), command: command_line(cli, &short, research, model), exit: None, secs, stderr_tail: String::new() };
    let res = match r {
        Ok(None) => CliResult::Timeout,
        Ok(Some(ran)) => {
            trial.exit = Some(ran.code);
            trial.stderr_tail = tail(&strip_ansi(&ran.stderr), 400);
            // codex writes its final message to the -o file; the others print it.
            let text = strip_ansi(if cli.id == "codex" { &file_text } else { &ran.stdout });
            if ran.code == 0 && !text.trim().is_empty() {
                CliResult::Answered(text.trim().to_string())
            } else {
                CliResult::Failed(explain(cli, ran.code, &ran.stdout, &ran.stderr))
            }
        }
        Err(e) => CliResult::Failed(Failure { fix: Fix::Retry, message: format!("{} could not start: {e}", cli.name) }),
    };
    (res, trial)
}

/// A model to try when the tool's default fails: OpenCode only, the first model
/// from a provider `opencode auth list` says is signed in. Names only, never secrets.
pub fn fallback_model(cli: &Cli, exe: &Path) -> Option<String> {
    if cli.id != "opencode" {
        return None;
    }
    let run = |args: &[&str]| -> Option<String> {
        let mut c = tool_command(exe);
        c.args(args).current_dir(std::env::temp_dir());
        run_with_timeout(c, None, Duration::from_secs(15)).ok().flatten().filter(|r| r.code == 0).map(|r| strip_ansi(&r.stdout))
    };
    pick_opencode_model(&run(&["auth", "list"])?, &run(&["models"])?)
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase()
}

/// From `opencode auth list` and `opencode models`, the first model of a signed-in provider.
pub fn pick_opencode_model(auth_list: &str, models: &str) -> Option<String> {
    let signed: Vec<String> = auth_list
        .lines()
        .filter(|l| {
            let t = l.to_ascii_lowercase();
            t.contains("stored") || t.contains("oauth") || t.contains("api key")
        })
        .filter_map(|l| l.split("  ").map(str::trim).find(|p| !p.is_empty()).map(squash))
        .collect();
    models
        .lines()
        .map(str::trim)
        .filter(|m| m.contains('/') && !m.contains(' '))
        .find(|m| {
            let provider = squash(m.split('/').next().unwrap_or(""));
            !provider.is_empty() && signed.contains(&provider)
        })
        .map(str::to_string)
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

// ---- Signing in ----

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A Terminal script (Mac `.command`) that starts the tool's own sign-in.
pub fn signin_script(cli: &Cli, exe: &Path, path: &OsString) -> String {
    let mut cmd = sh_quote(&exe.to_string_lossy());
    for a in cli.login {
        cmd.push(' ');
        cmd.push_str(&sh_quote(a));
    }
    format!(
        "#!/bin/sh\nexport PATH={}\nclear\necho {}\necho\n{}\necho\necho {}\n",
        sh_quote(&path.to_string_lossy()),
        sh_quote(&format!("Signing in to {}. Follow the steps below.", cli.name)),
        cmd,
        sh_quote("When it says you are signed in, close this window and go back to Roundtable."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("2.1.321 (Claude Code)\n").as_deref(), Some("2.1.321"));
        assert_eq!(parse_version("codex-cli 0.167.0").as_deref(), Some("0.167.0"));
        assert_eq!(parse_version("grok 1.0.77 (50ca579fa996) [stable]").as_deref(), Some("1.0.77"));
        assert_eq!(parse_version("v0.67.0").as_deref(), Some("0.67.0"));
        assert_eq!(parse_version("2026.11.05-86047df").as_deref(), Some("2026.11.05-86047df"));
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn every_head_has_a_known_tool_or_none() {
        for c in CLIS {
            assert!(crate::providers::find(c.head).is_some(), "{} seats unknown head {}", c.id, c.head);
            assert!(!c.plan.contains('\u{2014}'));
        }
        assert_eq!(for_head("codex").unwrap().id, "codex");
        assert!(for_head("glm").is_none());
    }

    #[test]
    fn codex_and_grok_use_the_bash_flags() {
        let (a, via) = headless_args(find("codex").unwrap(), "Q", false, Path::new("/tmp/last.md"));
        if !cfg!(windows) {
            let a: Vec<_> = a.iter().map(|s| s.to_string_lossy().into_owned()).collect();
            assert_eq!(a, ["exec", "--skip-git-repo-check", "-s", "read-only", "-c", "tools.web_search=false", "-o", "/tmp/last.md", "Q"]);
            assert_eq!(via, PromptVia::Arg);
            let (g, _) = headless_args(find("grok").unwrap(), "Q", true, Path::new("/tmp/x"));
            let g: Vec<_> = g.iter().map(|s| s.to_string_lossy().into_owned()).collect();
            assert_eq!(g, ["-p", "Q", "--max-turns", "8", "--no-subagents"]);
        }
        let (c, via) = headless_args(find("claude").unwrap(), "Q", false, Path::new("/x"));
        assert_eq!(via, PromptVia::Stdin);
        assert!(c.iter().any(|s| s == "--strict-mcp-config"), "never load Roundtable into itself");
        assert_eq!(how_line(find("claude").unwrap()), "claude -p --output-format text --strict-mcp-config --no-session-persistence --disallowedTools WebSearch,WebFetch < question");
    }

    fn args_of(id: &str, research: bool, model: Option<&str>) -> (Vec<String>, PromptVia) {
        let (a, via) = headless_args_with(find(id).unwrap(), "Q", research, Path::new("/tmp/last.md"), model);
        (a.iter().map(|s| s.to_string_lossy().into_owned()).collect(), via)
    }

    /// Each builder, against the flags read from that tool's own --help on a Mac
    /// (2026-11-10: claude 2.1.321, codex 0.167.0, gemini 0.67.0, grok 1.0.77,
    /// qwen 0.27.0, kimi 2.5.2, opencode 2.1.4, cursor-agent 2026.11.05).
    #[test]
    fn every_head_command_builder() {
        let (a, v) = args_of("claude", false, None);
        assert_eq!(a, ["-p", "--output-format", "text", "--strict-mcp-config", "--no-session-persistence", "--disallowedTools", "WebSearch,WebFetch"]);
        assert_eq!(v, PromptVia::Stdin);
        let (a, _) = args_of("claude", true, None);
        assert!(!a.iter().any(|x| x == "--disallowedTools"), "web search allowed in research mode");

        let (a, v) = args_of("gemini", false, None);
        assert_eq!(a, ["--skip-trust", "--output-format", "text"], "a fresh folder is untrusted; without --skip-trust gemini exits 55");
        assert_eq!(v, PromptVia::Stdin);

        let (a, v) = args_of("qwen", false, None);
        assert_eq!(a, ["--output-format", "text"]);
        assert_eq!(v, PromptVia::Stdin);

        let (a, v) = args_of("kimi", false, None);
        assert_eq!(a, ["-p", "Q", "--output-format", "text"]);
        assert!(!a.iter().any(|x| x == "--quiet" || x == "--plan"), "kimi 2.5 has no --quiet and refuses --plan with -p");
        assert_eq!(v, PromptVia::Arg);

        let (a, _) = args_of("opencode", false, None);
        assert_eq!(a, ["run", "Q"]);
        let (a, _) = args_of("opencode", false, Some("zai-coding-plan/glm-5.3"));
        assert_eq!(a, ["run", "-m", "zai-coding-plan/glm-5.3", "Q"]);

        let (a, v) = args_of("cursor", false, None);
        assert_eq!(a, ["-p", "--output-format", "text", "--trust", "--mode", "ask", "Q"]);
        assert_eq!(v, PromptVia::Arg);

        if !cfg!(windows) {
            let (a, _) = args_of("grok", false, None);
            assert_eq!(a, ["-p", "Q", "--max-turns", "6", "--no-subagents", "--disable-web-search"]);
            assert!(!a.iter().any(|x| x == "-m"), "the tool's own default model; rc4's fixed id no longer existed");
            let (a, _) = args_of("grok", false, Some("grok-4.7"));
            assert_eq!(&a[2..4], ["-m", "grok-4.7"]);
            let (a, v) = args_of("codex", true, None);
            assert_eq!(a, ["exec", "--skip-git-repo-check", "-s", "read-only", "-o", "/tmp/last.md", "Q"]);
            assert_eq!(v, PromptVia::Arg);
        }
        for c in CLIS {
            assert!(!how_line(c).contains('\u{2014}'));
        }
        assert_eq!(command_line(find("kimi").unwrap(), TEST_PROMPT, false, None), "kimi -p 'Answer in five words: is water wet?' --output-format text");
    }

    /// The reasons, from what each tool printed on this Mac on 2026-11-10.
    #[test]
    fn plain_reasons_from_real_tool_output() {
        let f = explain(find("cursor").unwrap(), 1, "", "Error: Authentication required. Please run 'agent login' first, or set CURSOR_API_KEY environment variable.");
        assert_eq!(f.fix, Fix::SignIn);
        let f = explain(find("kimi").unwrap(), 1, "", "kimi version 2.5.2\nerror: failed to run prompt: The provided authorization grant is invalid\nSee log: /x/kimi-code.log");
        assert_eq!((f.fix, f.message.as_str()), (Fix::SignIn, "Kimi says you are not signed in. Sign in to Kimi again."));
        let f = explain(find("kimi").unwrap(), 1, "", "error: failed to run prompt: provider managed:kimi-code has no credential configured\nSee log: /x");
        assert_eq!(f.fix, Fix::SignIn);
        let f = explain(find("qwen").unwrap(), 1, "", "[API Error: 401 Incorrect API key provided. For details, see: https://help.aliyun.com]");
        assert_eq!(f.fix, Fix::SignIn);
        let f = explain(find("gemini").unwrap(), 1, "", "Error authenticating: IneligibleTierError: This client is no longer supported for Gemini Code Assist for individuals.\n    at _doSetupUser (file:///x.js:1:1)");
        assert_eq!(f.fix, Fix::AddKey);
        assert!(f.message.starts_with("Google no longer lets Gemini CLI answer"));
        let f = explain(find("gemini").unwrap(), 41, "", "Ripgrep is not available.\nInvalid auth method selected.");
        assert_eq!(f.fix, Fix::SignIn);
        let f = explain(find("grok").unwrap(), 1, "", "Error: Couldn't set model 'grok-composer-2.5-fast': Invalid params: \"unknown model id\".");
        assert_eq!(f.message, "Grok does not offer the model it was asked to use.");
        let f = explain(find("opencode").unwrap(), 1, "", "\u{1b}[0m\n> build \u{b7} default\n\u{1b}[91m\u{1b}[1mError: \u{1b}[0mProvider request failed with HTTP 405: Method Not Allowed");
        assert_eq!((f.fix, f.message.as_str()), (Fix::Retry, "OpenCode stopped with an error: Error: Provider request failed with HTTP 405: Method Not Allowed"));
        let f = explain(find("codex").unwrap(), 2, "", "");
        assert_eq!(f.message, "Codex stopped without an answer (exit 2).");
        assert!(!f.message.contains('\u{2014}'));
    }

    #[test]
    fn opencode_falls_back_to_a_signed_in_provider() {
        let auth = "\u{2022} Z.AI Coding Plan  Z.AI Coding Plan            stored\n";
        let models = "hardware-box/default\nopencode/big-pickle\nzai-coding-plan/glm-5.3\nzai-coding-plan/glm-5.3-flash\n";
        assert_eq!(pick_opencode_model(auth, models).as_deref(), Some("zai-coding-plan/glm-5.3"));
        assert_eq!(pick_opencode_model("", models), None);
        assert_eq!(pick_opencode_model("Anthropic  oauth\n", "anthropic/claude-x\n").as_deref(), Some("anthropic/claude-x"));
    }

    #[test]
    fn gemini_old_sign_in_gets_the_google_auth_type() {
        let home = tempfile::tempdir().unwrap();
        let g = find("gemini").unwrap();
        std::fs::create_dir_all(home.path().join(".gemini")).unwrap();
        let set = |t: &str| std::fs::write(home.path().join(".gemini/settings.json"), t).unwrap();
        if std::env::var_os("GEMINI_API_KEY").is_some() || std::env::var_os("GOOGLE_GENAI_USE_GCA").is_some() {
            return;
        }
        set(r#"{"selectedAuthType": "oauth-personal"}"#);
        assert_eq!(headless_env(g, home.path()), vec![("GOOGLE_GENAI_USE_GCA", "true".to_string())]);
        set(r#"{"security": {"auth": {"selectedType": "gemini-api-key"}}}"#);
        assert!(headless_env(g, home.path()).is_empty(), "a choice in the new place stands");
        assert!(headless_env(find("claude").unwrap(), home.path()).is_empty());
    }

    #[test]
    fn search_dirs_include_install_folders_and_dedupe() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".nvm/versions/node/v20.1.0/bin")).unwrap();
        std::fs::create_dir_all(home.path().join(".nvm/versions/node/v22.3.0/bin")).unwrap();
        let p = std::env::join_paths(["/a", "/a"]).unwrap();
        let d = search_dirs_for(Some(p), home.path());
        assert_eq!(d[0], PathBuf::from("/a"));
        assert_eq!(d.iter().filter(|x| **x == PathBuf::from("/a")).count(), 1);
        assert!(d.contains(&home.path().join(".local/bin")));
        let v22 = d.iter().position(|x| x.ends_with("v22.3.0/bin")).unwrap();
        let v20 = d.iter().position(|x| x.ends_with("v20.1.0/bin")).unwrap();
        assert!(v22 < v20, "newest node first");
    }

    #[test]
    fn signin_script_quotes() {
        let s = signin_script(find("claude").unwrap(), Path::new("/Users/o'neil/.local/bin/claude"), &OsString::from("/x:/y"));
        assert!(s.starts_with("#!/bin/sh\nexport PATH='/x:/y'\n"));
        assert!(s.contains("'/Users/o'\\''neil/.local/bin/claude' 'auth' 'login'"));
        assert!(!s.contains('\u{2014}'));
    }

    #[cfg(unix)]
    mod unix {
        use super::super::*;
        use std::os::unix::fs::PermissionsExt;

        pub fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
            let p = dir.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        }

        #[test]
        fn finds_only_executables() {
            let d = tempfile::tempdir().unwrap();
            std::fs::write(d.path().join("gemini"), "not executable").unwrap();
            let dirs = vec![d.path().to_path_buf()];
            assert!(find_in("gemini", &dirs).is_none());
            script(d.path(), "gemini", "exit 0");
            assert_eq!(find_in("gemini", &dirs), Some(d.path().join("gemini")));
        }

        #[test]
        fn detects_claude_signed_in_from_its_status_command() {
            let bin = tempfile::tempdir().unwrap();
            let home = tempfile::tempdir().unwrap();
            let exe = script(
                bin.path(),
                "claude",
                r#"case "$1" in --version) echo "2.1.321 (Claude Code)";; auth) echo '{"loggedIn": true, "authMethod": "claude.ai"}';; *) echo "PROMPT SENT" >&2; exit 9;; esac"#,
            );
            let d = detect_at(find("claude").unwrap(), Some(exe), home.path());
            assert!(d.found);
            assert_eq!(d.version.as_deref(), Some("2.1.321"));
            assert_eq!(d.signed_in, Some(true));
            assert_eq!(d.note, "Uses your Claude plan.");
            assert_eq!(d.login, "claude auth login");
        }

        #[test]
        fn detects_codex_signed_out() {
            let bin = tempfile::tempdir().unwrap();
            let home = tempfile::tempdir().unwrap();
            let exe = script(bin.path(), "codex", r#"case "$1" in --version) echo "codex-cli 0.167.0";; login) echo "Not logged in"; exit 1;; esac"#);
            let d = detect_at(find("codex").unwrap(), Some(exe), home.path());
            assert_eq!(d.signed_in, Some(false));
            assert_eq!(d.version.as_deref(), Some("0.167.0"));
        }

        #[test]
        fn detects_by_credential_file_without_reading_it() {
            let bin = tempfile::tempdir().unwrap();
            let home = tempfile::tempdir().unwrap();
            let exe = script(bin.path(), "gemini", r#"echo 0.67.0"#);
            let cli = find("gemini").unwrap();
            assert_eq!(detect_at(cli, Some(exe.clone()), home.path()).signed_in, Some(false));
            std::fs::create_dir_all(home.path().join(".gemini")).unwrap();
            let cred = home.path().join(".gemini/oauth_creds.json");
            std::fs::write(&cred, "secret").unwrap();
            // Unreadable to prove the check never opens it.
            std::fs::set_permissions(&cred, std::fs::Permissions::from_mode(0o000)).unwrap();
            assert_eq!(detect_at(cli, Some(exe), home.path()).signed_in, Some(true));
            std::fs::set_permissions(&cred, std::fs::Permissions::from_mode(0o600)).unwrap();
        }

        #[test]
        fn not_found_is_plain() {
            let d = detect_at(find("kimi").unwrap(), None, Path::new("/nonexistent"));
            assert!(!d.found && d.version.is_none() && d.signed_in.is_none());
        }

        #[test]
        fn claude_reads_the_question_on_stdin() {
            let bin = tempfile::tempdir().unwrap();
            let exe = script(bin.path(), "claude", r#"q=$(cat); echo "answer to: $q (args: $*)""#);
            match ask_with(find("claude").unwrap(), &exe, "Bread or garden?", false, Duration::from_secs(10)) {
                CliResult::Answered(t) => {
                    assert!(t.starts_with("answer to: Bread or garden?"));
                    assert!(t.contains("--strict-mcp-config"));
                }
                other => panic!("{other:?}"),
            }
        }

        #[test]
        fn a_hung_tool_times_out() {
            let bin = tempfile::tempdir().unwrap();
            let exe = script(bin.path(), "qwen", "sleep 30");
            let t = Instant::now();
            assert_eq!(ask_with(find("qwen").unwrap(), &exe, "Q", false, Duration::from_millis(500)), CliResult::Timeout);
            assert!(t.elapsed() < Duration::from_secs(10));
        }

        #[test]
        fn a_signed_out_tool_says_sign_in() {
            let bin = tempfile::tempdir().unwrap();
            let exe = script(bin.path(), "cursor-agent", "echo 'not signed in' >&2; exit 3");
            match ask_with(find("cursor").unwrap(), &exe, "Q", false, Duration::from_secs(10)) {
                CliResult::Failed(f) => {
                    assert_eq!(f.fix, Fix::SignIn);
                    assert_eq!(f.message, "Cursor Agent says you are not signed in. Sign in to Cursor Agent again.");
                }
                other => panic!("{other:?}"),
            }
        }
    }
}
