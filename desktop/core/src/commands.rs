//! Tauri commands the UI calls. Keys pass straight through to the engine's stdin;
//! the app never stores or reads one back (spec D2).

use crate::claude;
use crate::engine_install::{self, RealProbe};
use roundtable_engine::{history, paths, settings::Settings, VERSION};
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

fn engine_json(args: &[&str]) -> Result<Value, String> {
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
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo { version: VERSION, os: std::env::consts::OS, arch: std::env::consts::ARCH }
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
pub fn refresh_engine_on_launch() {
    std::thread::spawn(|| {
        let dir = paths::engine_dir();
        if engine_install::installed_path(&dir).is_file() && engine_install::needs_install(&dir, VERSION, &RealProbe) {
            let _ = engine_install::install(engine_install::sidecar_path().as_deref(), &dir, VERSION, &RealProbe, &|_| {});
        }
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

// ---- Asking ----

#[derive(Clone, Serialize)]
struct RunEvent {
    run: u64,
    data: Value,
}

/// Start a roundtable. Events arrive on `rt-event` as `{run, data}`.
#[tauri::command]
pub fn ask_start(
    app: AppHandle,
    state: State<'_, AppState>,
    question: String,
    heads: Vec<String>,
    rounds: u32,
    research: bool,
    context_path: Option<String>,
    context_text: Option<String>,
) -> Result<u64, String> {
    if question.trim().is_empty() {
        return Err("Type a question first.".into());
    }
    let mut slot = state.run.lock().unwrap();
    if let Some((_, mut old)) = slot.take() {
        let _ = old.kill();
    }
    let csv = heads.join(",");
    let rounds_s = rounds.clamp(1, 10).to_string();
    let mut args: Vec<&str> = vec!["ask", "--events", "--source", "app", "--rounds", &rounds_s];
    if !csv.is_empty() {
        args.extend(["--heads", &csv]);
    }
    if research {
        args.push("--research");
    }
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

#[tauri::command]
pub fn ask_stop(state: State<'_, AppState>) {
    if let Some((_, mut c)) = state.run.lock().unwrap().take() {
        let _ = c.kill();
        let _ = c.wait();
    }
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

#[tauri::command]
pub fn history_delete(id: String) -> Result<(), String> {
    history::delete(&paths::history_dir(), &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_clear() -> Result<(), String> {
    history::clear(&paths::history_dir()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn settings_get() -> Settings {
    Settings::load()
}

#[tauri::command]
pub fn settings_set(settings: Settings) -> Result<(), String> {
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
