//! Screen 4, "Connect to Claude" (spec D3).
//!
//! Nothing here runs until the person presses a button. Detection only reads files.
//!
//! - Claude app: merge `mcpServers.roundtable` into `claude_desktop_config.json`, with a backup.
//! - Claude Code: `claude mcp add roundtable --scope user -- <engine> mcp serve`.
//!   If that fails, merge the same block into `~/.claude.json`.
//! - Plugin already installed: offer `claude plugin disable <id>`, so Claude sees one tool.

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SERVER_NAME: &str = "roundtable";
pub const REPO: &str = "frontier-infra/roundtable";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ConfigError {
    /// The file is not valid JSON. We leave it alone.
    InvalidJson,
    /// The top level, or `mcpServers`, is not an object.
    WrongShape,
}

impl ConfigError {
    pub fn message(&self) -> &'static str {
        match self {
            ConfigError::InvalidJson => "Claude's settings file is damaged, so Roundtable left it alone. Open Claude, then Settings, then Developer, and fix or reset it.",
            ConfigError::WrongShape => "Claude's settings file has an unexpected shape, so Roundtable left it alone.",
        }
    }
}

/// The two lines for someone using Claude Code on another computer.
/// Names come from `.claude-plugin/marketplace.json` at build time.
pub fn plugin_lines() -> [String; 2] {
    [
        format!("/plugin marketplace add {REPO}"),
        format!("/plugin install {}@{}", env!("RT_PLUGIN_NAME"), env!("RT_MARKETPLACE_NAME")),
    ]
}

pub fn plugin_id() -> String {
    format!("{}@{}", env!("RT_PLUGIN_NAME"), env!("RT_MARKETPLACE_NAME"))
}

fn server_block(engine: &Path, with_type: bool) -> Value {
    let mut b = json!({"command": engine.to_string_lossy(), "args": ["mcp", "serve"]});
    if with_type {
        b["type"] = json!("stdio");
        b["env"] = json!({});
    }
    b
}

fn parse_obj(existing: Option<&str>) -> Result<Map<String, Value>, ConfigError> {
    let text = existing.unwrap_or("").trim();
    if text.is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(ConfigError::WrongShape),
        Err(_) => Err(ConfigError::InvalidJson),
    }
}

fn merge(existing: Option<&str>, block: Value) -> Result<String, ConfigError> {
    let mut root = parse_obj(existing)?;
    let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
    let Value::Object(servers) = servers else { return Err(ConfigError::WrongShape) };
    servers.insert(SERVER_NAME.into(), block);
    Ok(serde_json::to_string_pretty(&Value::Object(root)).unwrap() + "\n")
}

/// Claude app config with our server added or updated. Other entries stay as they were.
pub fn merge_desktop_config(existing: Option<&str>, engine: &Path) -> Result<String, ConfigError> {
    merge(existing, server_block(engine, false))
}

/// `~/.claude.json` fallback when `claude mcp add` fails (same shape as `lib/install.sh`).
pub fn merge_claude_json(existing: Option<&str>, engine: &Path) -> Result<String, ConfigError> {
    merge(existing, server_block(engine, true))
}

/// Remove only our entry. Returns the new text and whether anything changed.
pub fn remove_server(existing: &str) -> Result<(String, bool), ConfigError> {
    let mut root = parse_obj(Some(existing))?;
    let removed = match root.get_mut("mcpServers") {
        Some(Value::Object(s)) => s.remove(SERVER_NAME).is_some(),
        Some(_) => return Err(ConfigError::WrongShape),
        None => false,
    };
    Ok((serde_json::to_string_pretty(&Value::Object(root)).unwrap() + "\n", removed))
}

/// True when the config points `roundtable` at this engine.
pub fn points_at(existing: Option<&str>, engine: &Path) -> bool {
    parse_obj(existing)
        .ok()
        .and_then(|m| m.get("mcpServers")?.get(SERVER_NAME)?.get("command")?.as_str().map(str::to_string))
        .is_some_and(|c| Path::new(&c) == engine)
}

/// Write `text` to `path`, keeping a dated copy of the old file next to it.
pub fn write_with_backup(path: &Path, text: &str) -> std::io::Result<Option<PathBuf>> {
    let mut backup = None;
    if path.is_file() {
        let b = path.with_file_name(format!(
            "{}.roundtable-backup-{}",
            path.file_name().unwrap().to_string_lossy(),
            roundtable_engine::history::now()
        ));
        std::fs::copy(path, &b)?;
        backup = Some(b);
    } else if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("json.roundtable-tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(backup)
}

// ---- Claude Code command lines ----

/// A command to run, built without running it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl CliCommand {
    /// A `.cmd` or `.bat` shim on Windows has to go through `cmd /C`.
    pub fn to_command(&self) -> Command {
        let ext = self.program.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
        let mut c = if cfg!(windows) && matches!(ext.as_deref(), Some("cmd") | Some("bat")) {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(&self.program);
            c
        } else {
            Command::new(&self.program)
        };
        c.args(&self.args);
        crate::engine_install::no_window(&mut c);
        c
    }
}

pub fn mcp_add(claude: &Path, engine: &Path) -> CliCommand {
    CliCommand {
        program: claude.into(),
        args: vec![
            "mcp".into(),
            "add".into(),
            SERVER_NAME.into(),
            "--scope".into(),
            "user".into(),
            "--".into(),
            engine.to_string_lossy().into_owned(),
            "mcp".into(),
            "serve".into(),
        ],
    }
}

pub fn mcp_remove(claude: &Path) -> CliCommand {
    CliCommand {
        program: claude.into(),
        args: ["mcp", "remove", SERVER_NAME, "--scope", "user"].map(String::from).to_vec(),
    }
}

pub fn plugin_disable(claude: &Path, id: &str) -> CliCommand {
    CliCommand { program: claude.into(), args: vec!["plugin".into(), "disable".into(), id.into()] }
}

/// Installed Roundtable plugin ids, from `~/.claude/plugins/installed_plugins.json`.
pub fn installed_plugin_ids(text: &str) -> Vec<String> {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let plugins = v.get("plugins").unwrap_or(&v);
    plugins
        .as_object()
        .map(|m| m.keys().filter(|k| k.starts_with("roundtable@")).cloned().collect())
        .unwrap_or_default()
}

// ---- Where things live ----

pub fn home() -> PathBuf {
    roundtable_engine::paths::home()
}

/// Claude app config files that exist or would be created, most likely first.
pub fn desktop_config_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(target_os = "macos") {
        v.push(home().join("Library/Application Support/Claude/claude_desktop_config.json"));
    } else if cfg!(windows) {
        if let Some(local) = dirs::data_local_dir() {
            // Microsoft Store build keeps its files in a package folder.
            if let Ok(rd) = std::fs::read_dir(local.join("Packages")) {
                for e in rd.flatten() {
                    if e.file_name().to_string_lossy().starts_with("Claude_") {
                        let p = e.path().join("LocalCache/Roaming/Claude/claude_desktop_config.json");
                        if p.parent().is_some_and(|d| d.is_dir()) {
                            v.push(p);
                        }
                    }
                }
            }
        }
        if let Some(roaming) = dirs::config_dir() {
            v.insert(0, roaming.join("Claude").join("claude_desktop_config.json"));
        }
    } else if let Some(c) = dirs::config_dir() {
        v.push(c.join("Claude").join("claude_desktop_config.json"));
    }
    v
}

pub fn claude_app_found() -> bool {
    let cfg_dir = desktop_config_paths().iter().any(|p| p.parent().is_some_and(|d| d.is_dir()));
    if cfg!(target_os = "macos") {
        cfg_dir || Path::new("/Applications/Claude.app").is_dir() || home().join("Applications/Claude.app").is_dir()
    } else if cfg!(windows) {
        cfg_dir || dirs::data_local_dir().is_some_and(|l| l.join("AnthropicClaude").is_dir())
    } else {
        cfg_dir
    }
}

pub fn claude_json_path() -> PathBuf {
    home().join(".claude.json")
}

pub fn installed_plugins_path() -> PathBuf {
    home().join(".claude/plugins/installed_plugins.json")
}

/// Find the `claude` command. A Mac app has a short PATH, so also ask the login shell.
pub fn find_claude_code() -> Option<PathBuf> {
    if let Some(p) = roundtable_engine::council::find_exe("claude") {
        return Some(p);
    }
    #[cfg(target_os = "macos")]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        if let Ok(o) = Command::new(shell).args(["-lc", "command -v claude"]).output() {
            let p = PathBuf::from(String::from_utf8_lossy(&o.stdout).trim());
            if p.is_absolute() && p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> PathBuf {
        PathBuf::from("/Users/a b/Library/Application Support/sh.roundtable.app/engine/roundtable")
    }

    #[test]
    fn mcp_add_command_line() {
        let c = mcp_add(Path::new("/opt/homebrew/bin/claude"), &engine());
        assert_eq!(c.program, PathBuf::from("/opt/homebrew/bin/claude"));
        assert_eq!(
            c.args,
            vec!["mcp", "add", "roundtable", "--scope", "user", "--", engine().to_str().unwrap(), "mcp", "serve"]
        );
    }

    #[test]
    fn engine_path_with_spaces_is_one_argument() {
        let c = mcp_add(Path::new("claude"), &engine());
        assert!(c.args.iter().any(|a| a.ends_with("engine/roundtable") && a.contains("a b")));
        let cmd = c.to_command();
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args.len(), 9);
    }

    #[test]
    fn remove_and_disable_command_lines() {
        assert_eq!(mcp_remove(Path::new("claude")).args, vec!["mcp", "remove", "roundtable", "--scope", "user"]);
        assert_eq!(plugin_disable(Path::new("claude"), "roundtable@frontier-infra").args, vec!["plugin", "disable", "roundtable@frontier-infra"]);
    }

    #[test]
    fn plugin_lines_match_marketplace_json() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.claude-plugin/marketplace.json");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let id = format!("{}@{}", v["plugins"][0]["name"].as_str().unwrap(), v["name"].as_str().unwrap());
        let lines = plugin_lines();
        assert_eq!(lines[0], "/plugin marketplace add frontier-infra/roundtable");
        assert_eq!(lines[1], format!("/plugin install {id}"));
        assert_eq!(plugin_id(), id);
    }

    #[test]
    fn merge_into_missing_or_empty_file() {
        for start in [None, Some(""), Some("  \n")] {
            let out = merge_desktop_config(start, &engine()).unwrap();
            let v: Value = serde_json::from_str(&out).unwrap();
            assert_eq!(v["mcpServers"]["roundtable"]["args"], json!(["mcp", "serve"]));
            assert_eq!(v["mcpServers"]["roundtable"]["command"], json!(engine().to_str().unwrap()));
        }
    }

    #[test]
    fn merge_keeps_other_servers_and_settings_in_order() {
        let start = r#"{"globalShortcut":"Cmd+K","mcpServers":{"files":{"command":"npx","args":["fs"]},"roundtable":{"command":"old"}},"z":1}"#;
        let out = merge_desktop_config(Some(start), &engine()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["globalShortcut"], "Cmd+K");
        assert_eq!(v["mcpServers"]["files"]["args"], json!(["fs"]));
        assert_eq!(v["mcpServers"]["roundtable"]["command"], json!(engine().to_str().unwrap()));
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, vec!["globalShortcut", "mcpServers", "z"]);
        assert!(points_at(Some(&out), &engine()));
        assert!(!points_at(Some(start), &engine()));
    }

    #[test]
    fn damaged_config_is_left_alone() {
        assert_eq!(merge_desktop_config(Some("{oops"), &engine()), Err(ConfigError::InvalidJson));
        assert_eq!(merge_desktop_config(Some("[1,2]"), &engine()), Err(ConfigError::WrongShape));
        assert_eq!(merge_desktop_config(Some(r#"{"mcpServers":[]}"#), &engine()), Err(ConfigError::WrongShape));
    }

    #[test]
    fn claude_json_block_has_type() {
        let out = merge_claude_json(Some(r#"{"numStartups":3}"#), &engine()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["numStartups"], 3);
        assert_eq!(v["mcpServers"]["roundtable"]["type"], "stdio");
    }

    #[test]
    fn disconnect_removes_only_ours() {
        let start = r#"{"mcpServers":{"files":{"command":"npx"},"roundtable":{"command":"x"}}}"#;
        let (out, changed) = remove_server(start).unwrap();
        assert!(changed);
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v["mcpServers"].get("roundtable").is_none());
        assert_eq!(v["mcpServers"]["files"]["command"], "npx");
        let (_, changed) = remove_server(&out).unwrap();
        assert!(!changed);
    }

    #[test]
    fn backup_then_write() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("Claude/claude_desktop_config.json");
        assert_eq!(write_with_backup(&p, "{}\n").unwrap(), None);
        let b = write_with_backup(&p, "{\"a\":1}\n").unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(b).unwrap(), "{}\n");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{\"a\":1}\n");
    }

    #[test]
    fn finds_installed_plugin() {
        let t = r#"{"version":2,"plugins":{"roundtable@frontier-infra":[{"scope":"user"}],"other@x":[]}}"#;
        assert_eq!(installed_plugin_ids(t), vec!["roundtable@frontier-infra"]);
        assert!(installed_plugin_ids("nonsense").is_empty());
    }
}
