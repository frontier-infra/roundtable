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

/// Arguments for one headless answer. `out_file` is where codex writes its last message.
pub fn headless_args(cli: &Cli, prompt: &str, research: bool, out_file: &Path) -> (Vec<OsString>, PromptVia) {
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
        // Same flags as the grok CLI fallback in lib/core.sh.
        "grok" => {
            let model = std::env::var("ROUNDTABLE_GROK_CLI_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| "grok-composer-2.5-fast".into());
            if cfg!(windows) {
                push("--prompt-file");
                a.push(out_file.with_extension("prompt").into_os_string());
            } else {
                push("-p");
                a.push(prompt.into());
            }
            let mut push = |s: &str| a.push(s.into());
            push("-m");
            push(&model);
            push("--max-turns");
            push(if research { "8" } else { "6" });
            if !research {
                push("--disable-web-search");
            }
            PromptVia::Arg
        }
        // Piped input runs these headless.
        "gemini" | "qwen" => {
            push("--output-format");
            push("text");
            PromptVia::Stdin
        }
        "kimi" => {
            for s in ["--quiet", "--plan", "-p"] {
                push(s);
            }
            a.push(prompt.into());
            PromptVia::Arg
        }
        "opencode" => {
            push("run");
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

/// The command line shown to people, with the question left out.
pub fn how_line(cli: &Cli) -> String {
    let (args, via) = headless_args(cli, "<question>", false, Path::new("<answer file>"));
    let mut s = cli.exes[0].to_string();
    for a in args {
        let a = a.to_string_lossy().into_owned();
        s.push(' ');
        s.push_str(&a);
    }
    if via == PromptVia::Stdin {
        s.push_str(" < question");
    }
    s
}

/// What came back from one headless run.
#[derive(Debug, Clone, PartialEq)]
pub enum CliResult {
    Answered(String),
    Missing,
    Timeout,
    Empty(String),
}

/// Ask one tool one question, headless, in an empty folder, with a time limit.
pub fn ask(cli: &Cli, prompt: &str, research: bool, timeout: Duration) -> CliResult {
    let Some(exe) = find_cli(cli) else { return CliResult::Missing };
    ask_with(cli, &exe, prompt, research, timeout)
}

pub fn ask_with(cli: &Cli, exe: &Path, prompt: &str, research: bool, timeout: Duration) -> CliResult {
    let dir = std::env::temp_dir().join(format!("roundtable-{}-{}-{}", cli.id, std::process::id(), nanos()));
    let work = dir.join("work");
    let _ = std::fs::create_dir_all(&work);
    let last = dir.join("last.md");
    let (args, via) = headless_args(cli, prompt, research, &last);
    if cli.id == "grok" && cfg!(windows) {
        let _ = std::fs::write(last.with_extension("prompt"), prompt);
    }
    let mut c = tool_command(exe);
    c.args(&args).current_dir(&work);
    let r = run_with_timeout(c, if via == PromptVia::Stdin { Some(prompt) } else { None }, timeout);
    let file_text = std::fs::read_to_string(&last).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    match r {
        Ok(None) => CliResult::Timeout,
        Ok(Some(ran)) => {
            // codex writes its final message to the -o file; the others print it.
            let text = if cli.id == "codex" { file_text } else { ran.stdout };
            if !text.trim().is_empty() {
                CliResult::Answered(text.trim().to_string())
            } else {
                let tail: String = ran.stderr.chars().rev().take(200).collect::<Vec<_>>().into_iter().rev().collect();
                CliResult::Empty(format!("{} rc={} {}", cli.id, ran.code, tail.trim()))
            }
        }
        Err(e) => CliResult::Empty(format!("{}: {e}", cli.id)),
    }
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
            assert_eq!(g, ["-p", "Q", "-m", "grok-composer-2.5-fast", "--max-turns", "8"]);
        }
        let (c, via) = headless_args(find("claude").unwrap(), "Q", false, Path::new("/x"));
        assert_eq!(via, PromptVia::Stdin);
        assert!(c.iter().any(|s| s == "--strict-mcp-config"), "never load Roundtable into itself");
        assert_eq!(how_line(find("claude").unwrap()), "claude -p --output-format text --strict-mcp-config --no-session-persistence --disallowedTools WebSearch,WebFetch < question");
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
        fn empty_output_reports_the_exit_code() {
            let bin = tempfile::tempdir().unwrap();
            let exe = script(bin.path(), "cursor-agent", "echo 'not signed in' >&2; exit 3");
            match ask_with(find("cursor").unwrap(), &exe, "Q", false, Duration::from_secs(10)) {
                CliResult::Empty(m) => assert!(m.contains("rc=3") && m.contains("not signed in")),
                other => panic!("{other:?}"),
            }
        }
    }
}
