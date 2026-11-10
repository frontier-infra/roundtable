//! Tauri commands the UI calls. Keys pass straight through to the engine's stdin;
//! the app never stores or reads one back (spec D2).

use crate::claude;
use crate::engine_install::{self, RealProbe};
use roundtable_engine::{clis, history, paths, personas, settings::Settings, VERSION};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct AppState {
    pub run: Mutex<Option<(u64, Child)>>,
    pub next_run: Mutex<u64>,
    /// One-head retries, so Stop ends them too.
    pub retries: std::sync::Arc<Mutex<Vec<Child>>>,
}

/// One install at a time (launch refresh and the status check can race).
static INSTALL: Mutex<()> = Mutex::new(());

/// Re-copy the engine when it is not the app's own build. Returns true when it did.
fn refresh_engine() -> bool {
    let _g = INSTALL.lock().unwrap_or_else(|e| e.into_inner());
    let dir = paths::engine_dir();
    let src = engine_install::sidecar_path();
    if engine_install::needs_refresh(&dir, src.as_deref(), VERSION, &RealProbe) {
        return engine_install::install(src.as_deref(), &dir, VERSION, &RealProbe, &|_| {}).is_ok();
    }
    false
}

fn installed() -> PathBuf {
    engine_install::installed_path(&paths::engine_dir())
}

/// The engine to run: the installed copy, else the one inside the app.
fn engine() -> Result<PathBuf, String> {
    let p = installed();
    if p.is_file() {
        return Ok(p);
    }
    engine_install::sidecar_path().filter(|p| p.is_file()).ok_or_else(|| "Roundtable is not set up yet.".to_string())
}

fn engine_cmd(args: &[&str]) -> Result<Command, String> {
    let mut c = Command::new(engine()?);
    c.args(args);
    engine_install::no_window(&mut c);
    Ok(c)
}

pub(crate) fn engine_json(args: &[&str]) -> Result<Value, String> {
    let o = engine_cmd(args)?.output().map_err(|e| e.to_string())?;
    serde_json::from_slice(&o.stdout).map_err(|_| "The engine gave an unexpected answer.".to_string())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
    /// Debug builds only: `ROUNDTABLE_START_SCREEN=3` or `settings` opens there (screenshots).
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<String>,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    let start = if cfg!(debug_assertions) { std::env::var("ROUNDTABLE_START_SCREEN").ok().filter(|v| !v.is_empty()) } else { None };
    AppInfo { version: VERSION, os: std::env::consts::OS, arch: std::env::consts::ARCH, start }
}

#[derive(Serialize)]
pub struct EngineStatus {
    installed: bool,
    current: bool,
    version: Option<String>,
    expected: &'static str,
}

#[tauri::command]
pub async fn engine_status() -> Result<EngineStatus, String> {
    blocking(|| {
        refresh_engine();
        let p = installed();
        let version = if p.is_file() { engine_install::engine_version(&p, &RealProbe) } else { None };
        EngineStatus { installed: p.is_file(), current: version.as_deref() == Some(VERSION), version, expected: VERSION }
    })
    .await
}

#[derive(Serialize)]
pub struct SetupFailure {
    code: &'static str,
    step: usize,
    detail: String,
}

/// Screen 2. Emits `setup-step` with the index of each tick as it turns green.
#[tauri::command]
pub async fn engine_setup(app: AppHandle) -> Result<(), SetupFailure> {
    blocking(move || {
        let _g = INSTALL.lock().unwrap_or_else(|e| e.into_inner());
        let src = engine_install::sidecar_path();
        engine_install::install(src.as_deref(), &paths::engine_dir(), VERSION, &RealProbe, &|i| {
            let _ = app.emit("setup-step", i);
        })
        .map(|_| ())
        .map_err(|e| SetupFailure { code: e.code(), step: e.step(), detail: e.detail().to_string() })
    })
    .await
    .map_err(|e| SetupFailure { code: "E_TASK", step: 0, detail: e })?
}

/// After an app update, quietly re-copy the engine when its version differs (spec 7.1).
/// Compares bytes, not only the version: release candidates share a version number.
pub fn refresh_engine_on_launch() {
    std::thread::spawn(|| {
        refresh_engine();
    });
}

#[tauri::command]
pub async fn keys_status() -> Result<Value, String> {
    blocking(|| engine_json(&["auth", "status", "--json"])).await?
}

/// Check the key with one small request, then save it in the keychain (through the engine).
#[tauri::command]
pub async fn key_save(head: String, key: String) -> Result<Value, String> {
    blocking(move || {
        let mut c = engine_cmd(&["auth", "set", &head, "--check", "--json"])?;
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = c.spawn().map_err(|e| e.to_string())?;
        if let Some(mut i) = child.stdin.take() {
            let _ = i.write_all(key.as_bytes());
        }
        drop(key);
        let o = child.wait_with_output().map_err(|e| e.to_string())?;
        serde_json::from_slice::<Value>(&o.stdout).map_err(|_| "The engine gave an unexpected answer.".to_string())
    })
    .await?
}

#[tauri::command]
pub async fn key_remove(head: String) -> Result<Value, String> {
    blocking(move || engine_json(&["auth", "remove", &head, "--json"])).await?
}

#[tauri::command]
pub async fn keys_import(remove: bool) -> Result<Value, String> {
    blocking(move || {
        let mut a = vec!["auth", "import", "--json"];
        if remove {
            a.push("--remove");
        }
        engine_json(&a)
    })
    .await?
}

#[tauri::command]
pub async fn doctor(online: bool) -> Result<Value, String> {
    blocking(move || {
        let mut a = vec!["doctor", "--json"];
        if online {
            a.push("--online");
        }
        engine_json(&a)
    })
    .await?
}

// ---- Providers the person adds (OpenRouter, Ollama, LM Studio, Groq, Together, Custom) ----

/// Added providers, their ticked seats and whether each has its key. Never a key.
#[tauri::command]
pub async fn providers_list() -> Result<Value, String> {
    blocking(|| engine_json(&["provider", "list", "--json"])).await?
}

/// Ollama and LM Studio running on this computer, with their models. Sends no prompt.
#[tauri::command]
pub async fn providers_detect() -> Result<Value, String> {
    blocking(|| engine_json(&["provider", "detect", "--json"])).await?
}

/// The models a provider lists, with prices where it publishes them.
#[tauri::command]
pub async fn provider_models(id: String) -> Result<Value, String> {
    blocking(move || engine_json(&["provider", "models", &id, "--json"])).await?
}

/// Add or change a provider. The key, when given, goes straight to the engine's stdin
/// and from there to the keychain; `models` sets the ticked models (each one a seat).
#[tauri::command]
pub async fn provider_add(preset: String, id: Option<String>, name: Option<String>, url: Option<String>, key: Option<String>, models: Option<Vec<String>>, check: bool) -> Result<Value, String> {
    blocking(move || {
        let mut a: Vec<String> = vec!["provider".into(), "add".into(), preset, "--json".into()];
        for (flag, v) in [("--id", id), ("--name", name), ("--url", url)] {
            if let Some(v) = v.filter(|v| !v.trim().is_empty()) {
                a.push(flag.into());
                a.push(v);
            }
        }
        if let Some(m) = models {
            a.push("--models".into());
            a.push(m.join(","));
        }
        let key = key.filter(|k| !k.trim().is_empty());
        if key.is_some() {
            a.push("--key-stdin".into());
        }
        if check {
            a.push("--check".into());
        }
        let args: Vec<&str> = a.iter().map(String::as_str).collect();
        let mut c = engine_cmd(&args)?;
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = c.spawn().map_err(|e| e.to_string())?;
        if let Some(mut i) = child.stdin.take() {
            if let Some(k) = &key {
                let _ = i.write_all(k.as_bytes());
            }
        }
        drop(key);
        let o = child.wait_with_output().map_err(|e| e.to_string())?;
        serde_json::from_slice::<Value>(&o.stdout).map_err(|_| "The engine gave an unexpected answer.".to_string())
    })
    .await?
}

/// Remove a provider, its seats and its saved key.
#[tauri::command]
pub async fn provider_remove(id: String) -> Result<Value, String> {
    blocking(move || engine_json(&["provider", "remove", &id, "--json"])).await?
}

// ---- AI tools already on this computer ----

/// Find Claude Code, Codex, Gemini CLI and the rest, and whether each is signed in.
/// Sends no prompt and reads no secret (see `clis.rs`).
#[tauri::command]
pub async fn clis_detect() -> Result<Vec<clis::Detection>, String> {
    blocking(clis::detect_all).await
}

/// Open the tool's own sign-in in a terminal window.
#[tauri::command]
pub async fn cli_login(id: String) -> Result<Value, String> {
    blocking(move || {
        let cli = clis::find(&id).ok_or("Unknown app.")?;
        let exe = clis::find_cli(cli).ok_or_else(|| format!("{} is not on this computer.", cli.name))?;
        open_signin_terminal(cli, &exe)?;
        Ok(json!({"ok": true, "message": "Finish signing in in the new window, then click Check again."}))
    })
    .await?
}

#[cfg(target_os = "macos")]
fn open_signin_terminal(cli: &clis::Cli, exe: &std::path::Path) -> Result<(), String> {
    let dir = paths::data_dir().join("tmp");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = dir.join(format!("sign-in-{}.command", cli.id));
    std::fs::write(&script, clis::signin_script(cli, exe, &clis::child_path(exe))).map_err(|e| e.to_string())?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    let ok = Command::new("open").args(["-a", "Terminal"]).arg(&script).status().map_err(|e| e.to_string())?.success();
    if ok {
        Ok(())
    } else {
        Err("Could not open Terminal.".into())
    }
}

#[cfg(windows)]
fn open_signin_terminal(cli: &clis::Cli, exe: &std::path::Path) -> Result<(), String> {
    // `start` opens a new console window that stays open (`cmd /k`) after the sign-in.
    let mut c = Command::new("cmd");
    c.args(["/c", "start", "Roundtable sign-in", "cmd", "/k"]).arg(exe).args(cli.login);
    c.env("PATH", clis::child_path(exe));
    engine_install::no_window(&mut c);
    c.spawn().map(|_| ()).map_err(|e| format!("Could not open a terminal window: {e}"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_signin_terminal(cli: &clis::Cli, exe: &std::path::Path) -> Result<(), String> {
    let line = std::iter::once(exe.to_string_lossy().into_owned()).chain(cli.login.iter().map(|s| s.to_string())).collect::<Vec<_>>().join(" ");
    for term in ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"] {
        if Command::new(term).args(["-e", &line]).env("PATH", clis::child_path(exe)).spawn().is_ok() {
            return Ok(());
        }
    }
    Err(format!("Open a terminal and run: {line}"))
}

/// Ask one AI a five-word question through the route a real run uses, and say how it went.
/// `via`: "auto" (its app first, then the key), "app" or "key".
#[tauri::command]
pub async fn head_test(head: String, via: Option<String>) -> Result<Value, String> {
    blocking(move || {
        let via = via.unwrap_or_else(|| "auto".into());
        let mut c = engine_cmd(&["test", &head, "--via", &via, "--timeout", "90", "--json"])?;
        c.stdin(Stdio::null()).stderr(Stdio::null());
        let o = c.output().map_err(|e| e.to_string())?;
        serde_json::from_slice::<Value>(&o.stdout).map_err(|_| "The engine gave an unexpected answer.".to_string())
    })
    .await?
}

// ---- Claude ----

#[derive(Serialize)]
pub struct ClaudeStatus {
    app_found: bool,
    app_connected: bool,
    code_found: bool,
    code_connected: bool,
    plugin_ids: Vec<String>,
    plugin_lines: [String; 2],
}

#[tauri::command]
pub async fn claude_status() -> Result<ClaudeStatus, String> {
    blocking(|| {
        let eng = installed();
        let read = |p: &PathBuf| std::fs::read_to_string(p).ok();
        ClaudeStatus {
            app_found: claude::claude_app_found(),
            app_connected: claude::desktop_config_paths().iter().any(|p| claude::points_at(read(p).as_deref(), &eng)),
            code_found: claude::find_claude_code().is_some(),
            code_connected: claude::points_at(read(&claude::claude_json_path()).as_deref(), &eng),
            plugin_ids: std::fs::read_to_string(claude::installed_plugins_path())
                .map(|t| claude::installed_plugin_ids(&t))
                .unwrap_or_default(),
            plugin_lines: claude::plugin_lines(),
        }
    })
    .await
}

fn need_engine() -> Result<PathBuf, String> {
    let p = installed();
    if p.is_file() {
        Ok(p)
    } else {
        Err("Finish setup first, then connect Claude.".into())
    }
}

#[tauri::command]
pub async fn claude_connect_app() -> Result<Value, String> {
    blocking(|| {
        let eng = need_engine()?;
        let paths = claude::desktop_config_paths();
        let target = paths
            .iter()
            .find(|p| p.is_file())
            .or_else(|| paths.iter().find(|p| p.parent().is_some_and(|d| d.is_dir())))
            .or(paths.first())
            .cloned()
            .ok_or("Could not find where the Claude app keeps its settings.")?;
        let existing = std::fs::read_to_string(&target).ok();
        let text = claude::merge_desktop_config(existing.as_deref(), &eng).map_err(|e| e.message().to_string())?;
        claude::write_with_backup(&target, &text).map_err(|e| format!("Could not save Claude's settings: {e}"))?;
        Ok(json!({"ok": true, "message": "Done. Quit and reopen Claude to finish."}))
    })
    .await?
}

#[tauri::command]
pub async fn claude_connect_code() -> Result<Value, String> {
    blocking(|| {
        let eng = need_engine()?;
        if let Some(cli) = claude::find_claude_code() {
            let _ = claude::mcp_remove(&cli).to_command().output();
            if let Ok(o) = claude::mcp_add(&cli, &eng).to_command().output() {
                if o.status.success() {
                    return Ok(json!({"ok": true, "method": "cli", "message": "Done. Start a new Claude Code session to use it."}));
                }
            }
        }
        let p = claude::claude_json_path();
        let existing = std::fs::read_to_string(&p).ok();
        let text = claude::merge_claude_json(existing.as_deref(), &eng).map_err(|e| e.message().to_string())?;
        claude::write_with_backup(&p, &text).map_err(|e| format!("Could not save Claude Code's settings: {e}"))?;
        Ok(json!({"ok": true, "method": "file", "message": "Done. Start a new Claude Code session to use it."}))
    })
    .await?
}

#[tauri::command]
pub async fn claude_disable_plugin() -> Result<Value, String> {
    blocking(|| {
        let cli = claude::find_claude_code().ok_or("Claude Code was not found on this computer.")?;
        let ids = std::fs::read_to_string(claude::installed_plugins_path())
            .map(|t| claude::installed_plugin_ids(&t))
            .unwrap_or_default();
        for id in &ids {
            let o = claude::plugin_disable(&cli, id).to_command().output().map_err(|e| e.to_string())?;
            if !o.status.success() {
                return Err(format!("Could not turn off the {id} plugin."));
            }
        }
        Ok(json!({"ok": true, "disabled": ids}))
    })
    .await?
}

#[tauri::command]
pub async fn claude_disconnect() -> Result<Value, String> {
    blocking(|| {
        let mut changed = 0;
        for p in claude::desktop_config_paths().into_iter().filter(|p| p.is_file()) {
            if let Ok(t) = std::fs::read_to_string(&p) {
                if let Ok((text, true)) = claude::remove_server(&t) {
                    claude::write_with_backup(&p, &text).map_err(|e| e.to_string())?;
                    changed += 1;
                }
            }
        }
        let mut cli_ok = false;
        if let Some(cli) = claude::find_claude_code() {
            cli_ok = claude::mcp_remove(&cli).to_command().output().is_ok_and(|o| o.status.success());
        }
        if !cli_ok {
            let p = claude::claude_json_path();
            if let Ok(t) = std::fs::read_to_string(&p) {
                if let Ok((text, true)) = claude::remove_server(&t) {
                    claude::write_with_backup(&p, &text).map_err(|e| e.to_string())?;
                    changed += 1;
                }
            }
        } else {
            changed += 1;
        }
        Ok(json!({"ok": true, "changed": changed}))
    })
    .await?
}

// ---- The question coach, suggested seats, files and links ----

/// The coach's clarifying questions (at most three). Runs the chair's seat once.
#[tauri::command]
pub async fn coach_questions(question: String) -> Result<Value, String> {
    blocking(move || engine_json(&["coach", "questions", "--json", "-q", &question])).await?
}

/// The brief from the question and the answers. `qa` is `[{q, a}]`.
#[tauri::command]
pub async fn coach_brief(question: String, qa: Vec<Value>) -> Result<Value, String> {
    blocking(move || {
        let mut c = engine_cmd(&["coach", "brief", "--json"])?;
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = c.spawn().map_err(|e| e.to_string())?;
        if let Some(mut i) = child.stdin.take() {
            let _ = i.write_all(json!({"question": question, "qa": qa}).to_string().as_bytes());
        }
        let o = child.wait_with_output().map_err(|e| e.to_string())?;
        serde_json::from_slice::<Value>(&o.stdout).map_err(|_| "The engine gave an unexpected answer.".to_string())
    })
    .await?
}

/// Seats suggested for a question and its brief, placed on these AIs.
#[tauri::command]
pub async fn suggest_panel(question: String, brief: Option<String>, heads: Vec<String>) -> Result<Value, String> {
    blocking(move || {
        let csv = heads.join(",");
        let mut args: Vec<&str> = vec!["suggest", "--json", "-q", &question];
        if !csv.is_empty() {
            args.extend(["--heads", &csv]);
        }
        let bf = match brief.filter(|b| !b.trim().is_empty()) {
            Some(b) => Some(tmp_file("suggest-brief.md", &b)?),
            None => None,
        };
        if let Some(p) = bf.as_deref() {
            args.extend(["--brief", p]);
        }
        engine_json(&args)
    })
    .await?
}

/// Read a file or link once, as every seat will read it: whole, or summarized once.
#[tauri::command]
pub async fn attach_prepare(source: String) -> Result<Value, String> {
    blocking(move || engine_json(&["attach", "--json", &source])).await?
}

/// Write text the engine reads for one run into the app's tmp folder.
fn tmp_file(name: &str, text: &str) -> Result<String, String> {
    let dir = paths::data_dir().join("tmp");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join(name);
    std::fs::write(&p, text).map_err(|e| e.to_string())?;
    Ok(p.to_string_lossy().into_owned())
}

/// The run's extra text, as engine flags: the brief, earlier turns and shared material.
fn extra_args(prefix: &str, brief: Option<String>, prior: Option<String>, attached: Option<String>) -> Result<Vec<String>, String> {
    let mut v = Vec::new();
    for (flag, name, text) in [("--brief", "brief", brief), ("--follow-up", "prior", prior), ("--attached", "attached", attached)] {
        if let Some(t) = text.filter(|t| !t.trim().is_empty()) {
            v.push(flag.to_string());
            v.push(tmp_file(&format!("{prefix}{name}.md"), &t)?);
        }
    }
    Ok(v)
}

// ---- Asking ----

#[derive(Clone, Serialize)]
struct RunEvent {
    run: u64,
    data: Value,
}

/// Start a roundtable. Events arrive on `rt-event` as `{run, data}`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn ask_start(
    app: AppHandle,
    state: State<'_, AppState>,
    question: String,
    heads: Vec<String>,
    rounds: u32,
    mode: Option<String>,
    research: bool,
    context_path: Option<String>,
    context_text: Option<String>,
    personas: Option<String>,
    brief: Option<String>,
    verdict: Option<String>,
    open_first_round: Option<bool>,
    prior: Option<String>,
    attached: Option<String>,
    source: Option<String>,
) -> Result<u64, String> {
    if question.trim().is_empty() {
        return Err("Type a question first.".into());
    }
    // The first-run sample is the setup check: the engine keeps it out of history.
    let source = match source.as_deref() {
        Some("setup") => "setup",
        _ => "app",
    };
    let mut slot = state.run.lock().unwrap();
    if let Some((_, mut old)) = slot.take() {
        let _ = old.kill();
    }
    let csv = heads.join(",");
    let rounds_s = rounds.clamp(1, roundtable_engine::council::MAX_ROUNDS).to_string();
    let mut args: Vec<&str> = vec!["ask", "--events", "--source", source, "--rounds", &rounds_s];
    // Quick or Deep Think: the answer length and the verdict. The rounds above come from the mode too.
    let depth = roundtable_engine::mode::Mode::parse(mode.as_deref().unwrap_or("")).map_err(|e| e.to_string())?;
    args.extend(["--mode", depth.id()]);
    if !csv.is_empty() {
        args.extend(["--heads", &csv]);
    }
    let spec = personas.unwrap_or_default();
    if !spec.trim().is_empty() {
        args.extend(["--personas", &spec]);
    }
    if research {
        args.push("--research");
    }
    let shape = verdict.unwrap_or_default();
    if !shape.trim().is_empty() {
        args.extend(["--verdict", &shape]);
    }
    if open_first_round == Some(true) {
        args.push("--no-blind");
    }
    let extra = extra_args("", brief, prior, attached)?;
    args.extend(extra.iter().map(String::as_str));
    // A follow-up passes the last transcript as text; the engine reads background from a file.
    let followup = match context_text.filter(|t| !t.trim().is_empty()) {
        Some(t) => {
            let dir = paths::data_dir().join("tmp");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let p = dir.join("follow-up.md");
            std::fs::write(&p, t).map_err(|e| e.to_string())?;
            Some(p.to_string_lossy().into_owned())
        }
        None => None,
    };
    if let Some(c) = followup.as_deref().or(context_path.as_deref().filter(|c| !c.is_empty())) {
        args.extend(["-c", c]);
    }
    args.extend(["-q", &question]);
    let mut cmd = engine_cmd(&args)?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("Could not start Roundtable: {e}"))?;
    let out = child.stdout.take().ok_or("no output")?;
    let id = {
        let mut n = state.next_run.lock().unwrap();
        *n += 1;
        *n
    };
    *slot = Some((id, child));
    drop(slot);
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                let _ = app.emit("rt-event", RunEvent { run: id, data: v });
            }
        }
        let _ = app.emit("rt-event", RunEvent { run: id, data: json!({"event": "process_exit"}) });
    });
    Ok(id)
}

/// Ask one head again, alone, without touching the rest of the run. Its events
/// arrive on `rt-event` with the same run id, plus `retry_done` with its transcript.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn ask_retry(
    app: AppHandle,
    state: State<'_, AppState>,
    run: u64,
    head: String,
    question: String,
    research: bool,
    context_path: Option<String>,
    seat: Option<String>,
    persona: Option<String>,
    brief: Option<String>,
    prior: Option<String>,
    attached: Option<String>,
) -> Result<(), String> {
    if question.trim().is_empty() {
        return Err("Type a question first.".into());
    }
    let seat = seat.unwrap_or_else(|| head.clone());
    let mut args: Vec<&str> = vec!["ask", "--events", "--no-history", "--source", "app", "--rounds", "1", "--heads", &head];
    // A persona seat asks again with the same persona on the same AI.
    let spec = persona.filter(|p| !p.trim().is_empty()).map(|p| format!("{p}@{head}"));
    if let Some(sp) = spec.as_deref() {
        args.extend(["--personas", sp]);
    }
    if research {
        args.push("--research");
    }
    // A follow-up run kept its background in tmp/follow-up.md.
    let followup = paths::data_dir().join("tmp").join("follow-up.md").to_string_lossy().into_owned();
    let ctx = context_path.filter(|c| !c.is_empty()).map(|c| if c == "follow-up" { followup.clone() } else { c });
    if let Some(c) = ctx.as_deref() {
        args.extend(["-c", c]);
    }
    let extra = extra_args(&format!("retry-{run}-{}-", seat.replace(['@', '#', '/', '\\', ':'], "_")), brief, prior, attached)?;
    args.extend(extra.iter().map(String::as_str));
    args.extend(["-q", &question]);
    let mut cmd = engine_cmd(&args)?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::null()).stdin(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("Could not start Roundtable: {e}"))?;
    let out = child.stdout.take().ok_or("no output")?;
    let pid = child.id();
    let retries = state.retries.clone();
    retries.lock().unwrap().push(child);
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let Ok(mut v) = serde_json::from_str::<Value>(&line) else { continue };
            match v["event"].as_str() {
                Some("head_started") | Some("head_done") => v["seat"] = json!(seat),
                Some("run_done") => v = json!({"event": "retry_done", "head": head, "seat": seat, "transcript": v["transcript"]}),
                _ => continue,
            }
            let _ = app.emit("rt-event", RunEvent { run, data: v });
        }
        let mut list = retries.lock().unwrap();
        if let Some(i) = list.iter().position(|c| c.id() == pid) {
            let mut c = list.remove(i);
            let _ = c.wait();
        }
    });
    Ok(())
}

#[tauri::command]
pub fn ask_stop(state: State<'_, AppState>) {
    if let Some((_, mut c)) = state.run.lock().unwrap().take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    for mut c in state.retries.lock().unwrap().drain(..) {
        let _ = c.kill();
        let _ = c.wait();
    }
}

// ---- Personas (Settings, Personas) ----

/// Packs, presets and the switch state, for the Seats control and the Personas settings.
#[tauri::command]
pub fn personas_list() -> Value {
    personas::library_json(&Settings::load())
}

/// Create (empty id) or update one of the person's own personas.
#[tauri::command]
pub fn persona_save(persona: personas::Persona) -> Result<personas::Persona, String> {
    let lib = personas::Library::load(&Settings::load());
    personas::save_custom_at(&personas::custom_file(), &lib, persona)
}

/// Copy any persona into the person's own pack, to edit.
#[tauri::command]
pub fn persona_duplicate(id: String) -> Result<personas::Persona, String> {
    let lib = personas::Library::load(&Settings::load());
    personas::duplicate_at(&personas::custom_file(), &lib, &id)
}

#[tauri::command]
pub fn persona_delete(id: String) -> Result<(), String> {
    personas::delete_custom_at(&personas::custom_file(), &id)
}

/// Write the person's own pack to a file they picked. Returns how many personas.
#[tauri::command]
pub fn personas_export(path: String) -> Result<usize, String> {
    personas::export_at(&personas::custom_file(), std::path::Path::new(&path))
}

/// Add every persona in a pack file to the person's own pack. Returns how many.
#[tauri::command]
pub fn personas_import(path: String) -> Result<usize, String> {
    let lib = personas::Library::load(&Settings::load());
    personas::import_at(&personas::custom_file(), &lib, std::path::Path::new(&path))
}


// ---- History and settings ----

#[tauri::command]
pub fn history_list() -> Vec<history::Entry> {
    history::list(&paths::history_dir())
}

#[tauri::command]
pub fn history_get(id: String) -> Option<String> {
    history::read(&paths::history_dir(), &id)
}

/// Take roundtables out of history. They wait aside until Undo ([`history_restore`])
/// or the end of the Undo window ([`history_purge`]). Returns the Undo token.
#[tauri::command]
pub fn history_delete(ids: Vec<String>) -> Result<String, String> {
    history::trash(&paths::history_dir(), &ids).map_err(|e| e.to_string())
}

/// Undo a delete: the roundtables come back exactly as they were.
#[tauri::command]
pub fn history_restore(token: String) -> Result<usize, String> {
    history::restore(&paths::history_dir(), &token).map_err(|e| e.to_string())
}

/// The Undo window closed: delete the files from this computer for good.
#[tauri::command]
pub fn history_purge(state: State<'_, AppState>, token: String) -> Result<(), String> {
    history::purge(&paths::history_dir(), &token).map_err(|e| e.to_string())?;
    clear_run_inputs(&state);
    Ok(())
}

#[tauri::command]
pub fn history_clear(state: State<'_, AppState>) -> Result<(), String> {
    history::clear(&paths::history_dir()).map_err(|e| e.to_string())?;
    clear_run_inputs(&state);
    Ok(())
}

/// The text a run read (attached-file summaries, the brief, earlier turns) sits in
/// tmp/ while it runs. With no run going, it goes, so a deleted roundtable leaves nothing.
fn clear_run_inputs(state: &AppState) {
    let alive = |c: &mut Child| matches!(c.try_wait(), Ok(None));
    let running = state.run.lock().map(|mut r| r.as_mut().is_some_and(|(_, c)| alive(c))).unwrap_or(true)
        || state.retries.lock().map(|mut v| v.iter_mut().any(alive)).unwrap_or(true);
    if !running {
        remove_run_inputs(&paths::data_dir().join("tmp"));
    }
}

/// Remove the run inputs (`*.md`) in `dir`. Sign-in scripts stay.
pub fn remove_run_inputs(dir: &std::path::Path) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().extension().is_some_and(|x| x == "md") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// On launch: finish deletes whose Undo window was cut short by a quit.
pub fn purge_history_trash() {
    let _ = history::purge_all(&paths::history_dir());
}

#[tauri::command]
pub fn settings_get() -> Settings {
    Settings::load()
}

#[tauri::command]
pub fn settings_set(settings: Settings) -> Result<(), String> {
    // Added providers belong to `roundtable provider`: a settings save from the screen never
    // drops or rolls back one it added a moment ago.
    let mut settings = settings;
    settings.providers = Settings::load().providers;
    settings.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_text(path: String, text: String) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// "Use Roundtable in Terminal too". Mac: a link in ~/.local/bin, only if that name is free.
/// Windows: the engine folder on the user PATH.
#[tauri::command]
pub async fn terminal_command(on: bool) -> Result<String, String> {
    blocking(move || {
        let eng = need_engine()?;
        #[cfg(unix)]
        {
            let link = paths::home().join(".local/bin/roundtable");
            let ours = std::fs::read_link(&link).is_ok_and(|t| t == eng);
            if on {
                if ours {
                    return Ok("Ready. Open a new Terminal window and type roundtable.".into());
                }
                if link.exists() || link.symlink_metadata().is_ok() {
                    return Err("A roundtable command is already installed, so Roundtable left it alone.".into());
                }
                std::fs::create_dir_all(link.parent().unwrap()).map_err(|e| e.to_string())?;
                std::os::unix::fs::symlink(&eng, &link).map_err(|e| e.to_string())?;
                Ok("Ready. Open a new Terminal window and type roundtable.".into())
            } else {
                if ours {
                    std::fs::remove_file(&link).map_err(|e| e.to_string())?;
                }
                Ok("Removed.".into())
            }
        }
        #[cfg(windows)]
        {
            let dir = eng.parent().unwrap().to_string_lossy().replace('\'', "''");
            let script = if on {
                format!("$p=[Environment]::GetEnvironmentVariable('Path','User'); if (-not ($p -split ';' -contains '{dir}')) {{ [Environment]::SetEnvironmentVariable('Path', (($p.TrimEnd(';')) + ';{dir}').TrimStart(';'), 'User') }}")
            } else {
                format!("$p=[Environment]::GetEnvironmentVariable('Path','User'); [Environment]::SetEnvironmentVariable('Path', (($p -split ';' | Where-Object {{ $_ -ne '{dir}' -and $_ -ne '' }}) -join ';'), 'User')")
            };
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
            engine_install::no_window(&mut c);
            let o = c.output().map_err(|e| e.to_string())?;
            if !o.status.success() {
                return Err("Could not change the PATH.".into());
            }
            Ok(if on { "Ready. Open a new terminal window and type roundtable." } else { "Removed." }.into())
        }
    })
    .await?
}

/// Versions, OS and AI status. Never keys.
#[tauri::command]
pub async fn diagnostics() -> Result<String, String> {
    blocking(|| {
        let d = engine_json(&["doctor", "--json"]).unwrap_or(json!({"error": "engine not available"}));
        let heads: Vec<String> = d["heads"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|h| format!("  {}: {} ({})", h["head"].as_str().unwrap_or(""), if h["has_key"] == json!(true) { "key set" } else { "no key" }, h["model"].as_str().unwrap_or("")))
                    .collect()
            })
            .unwrap_or_default();
        format!(
            "Roundtable {VERSION}\nOS: {} {}\nEngine: {}\nAIs:\n{}\n",
            std::env::consts::OS,
            std::env::consts::ARCH,
            d["version"].as_str().unwrap_or("not set up"),
            heads.join("\n")
        )
    })
    .await
}
