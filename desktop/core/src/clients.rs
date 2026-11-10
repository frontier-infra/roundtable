//! One-click connect for AI apps other than Claude: Codex CLI and Cursor.
//! Plus the copyable snippets for any other MCP client.
//!
//! - Codex CLI: our `[mcp_servers.roundtable]` table in `~/.codex/config.toml` (or
//!   `$CODEX_HOME/config.toml`), with `tool_timeout_sec` raised: Codex stops a tool
//!   after 60 s by default and a roundtable takes minutes. Other tables stay as they were.
//! - Cursor: `mcpServers.roundtable` in `~/.cursor/mcp.json`, the Claude app's shape.
//!
//! Nothing runs until the person presses a button. Every write keeps a dated backup.

use roundtable_engine::mcp;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const SERVER_NAME: &str = "roundtable";

// ---- Codex ----

/// The table name a header line opens, without brackets, spaces or quotes.
fn table_name(line: &str) -> Option<String> {
    let t = line.trim();
    if t.starts_with("[[") {
        return Some(t.trim_start_matches('[').trim_end_matches(']').replace([' ', '"', '\''], ""));
    }
    let inner = t.strip_prefix('[')?;
    let end = inner.find(']')?;
    Some(inner[..end].replace([' ', '"', '\''], ""))
}

fn ours(name: &str) -> bool {
    name == "mcp_servers.roundtable" || name.starts_with("mcp_servers.roundtable.")
}

/// Our server written some other way (a dotted key or an inline table), which we leave alone.
fn defined_inline(text: &str) -> bool {
    let mut table = String::new();
    for line in text.lines() {
        if let Some(n) = table_name(line) {
            table = n;
            continue;
        }
        let key = line.split('=').next().unwrap_or("").replace([' ', '"', '\''], "");
        if !line.contains('=') || line.trim_start().starts_with('#') {
            continue;
        }
        let full = if table.is_empty() { key } else { format!("{table}.{key}") };
        if full == "mcp_servers.roundtable" || (full.starts_with("mcp_servers.roundtable.") && !ours(&table)) {
            return true;
        }
    }
    false
}

/// The text without our tables. Returns the text and whether anything was removed.
fn strip_ours(text: &str) -> (String, bool) {
    let mut out = Vec::new();
    let mut skipping = false;
    let mut removed = false;
    for line in text.lines() {
        if let Some(n) = table_name(line) {
            skipping = ours(&n);
            removed |= skipping;
        }
        if !skipping {
            out.push(line);
        }
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    (s, removed)
}

/// Codex config with our server added or updated.
pub fn merge_codex(existing: Option<&str>, engine: &Path) -> Result<String, String> {
    let text = existing.unwrap_or("");
    if defined_inline(text) {
        return Err("Codex's config.toml names roundtable in a way Roundtable does not edit, so it left the file alone. Remove the roundtable entry from ~/.codex/config.toml and try again.".into());
    }
    let (mut s, _) = strip_ours(text);
    if !s.is_empty() {
        s.push('\n');
    }
    s.push_str(&mcp::codex_toml(&engine.to_string_lossy()));
    Ok(s)
}

pub fn remove_codex(existing: &str) -> (String, bool) {
    strip_ours(existing)
}

/// True when the config runs this engine as `roundtable`.
pub fn codex_points_at(existing: Option<&str>, engine: &Path) -> bool {
    let want = format!("command = {}", mcp::toml_string(&engine.to_string_lossy()));
    let mut inside = false;
    for line in existing.unwrap_or("").lines() {
        if let Some(n) = table_name(line) {
            inside = n == "mcp_servers.roundtable";
            continue;
        }
        if inside && line.split_whitespace().collect::<Vec<_>>().join(" ") == want {
            return true;
        }
    }
    false
}

pub fn codex_config_path() -> PathBuf {
    match std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()) {
        Some(h) => PathBuf::from(h).join("config.toml"),
        None => crate::claude::home().join(".codex/config.toml"),
    }
}

pub fn codex_found() -> bool {
    codex_config_path().parent().is_some_and(|d| d.is_dir()) || roundtable_engine::council::find_exe("codex").is_some()
}

// ---- Cursor ----

pub fn cursor_config_path() -> PathBuf {
    crate::claude::home().join(".cursor/mcp.json")
}

pub fn cursor_found() -> bool {
    if cursor_config_path().parent().is_some_and(|d| d.is_dir()) {
        return true;
    }
    if cfg!(target_os = "macos") {
        return Path::new("/Applications/Cursor.app").is_dir() || crate::claude::home().join("Applications/Cursor.app").is_dir();
    }
    if cfg!(windows) {
        return dirs::data_local_dir().is_some_and(|l| l.join("Programs/cursor").is_dir());
    }
    false
}

/// Cursor's file has the Claude app's shape. Its errors name Cursor.
pub fn merge_cursor(existing: Option<&str>, engine: &Path) -> Result<String, String> {
    crate::claude::merge_desktop_config(existing, engine).map_err(|e| match e {
        crate::claude::ConfigError::InvalidJson => "Cursor's mcp.json is damaged, so Roundtable left it alone. Fix it in Cursor's MCP settings and try again.".to_string(),
        crate::claude::ConfigError::WrongShape => "Cursor's mcp.json has an unexpected shape, so Roundtable left it alone.".to_string(),
    })
}

// ---- Snippets for any other client ----

/// Copyable setup for each client, for the app's "Other AI apps" list.
pub fn snippets(engine: &Path) -> Value {
    let e = engine.to_string_lossy();
    let s = |c: &str| mcp::config_snippet(c, &e).unwrap_or_default();
    json!([
        {"id": "generic", "name": "Any MCP app (JSON)", "where": "The app's MCP settings, under mcpServers. Windsurf: ~/.codeium/windsurf/mcp_config.json.", "text": s("generic")},
        {"id": "claude-code", "name": "Claude Code (one command)", "where": "Run in a terminal.", "text": s("claude-code")},
        {"id": "codex", "name": "Codex CLI", "where": "Add to ~/.codex/config.toml.", "text": s("codex")},
        {"id": "vscode", "name": "VS Code", "where": "Add to .vscode/mcp.json, or run MCP: Add Server.", "text": s("vscode")},
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> PathBuf {
        PathBuf::from("/Users/a b/Library/Application Support/sh.roundtable.app/engine/roundtable")
    }

    const CODEX: &str = "model = \"gpt-5\"\n\n[mcp_servers.linear]\nurl = \"https://mcp.linear.app/mcp\"\n\n[mcp_servers.linear.tools.save_comment]\napproval = \"never\"\n\n[profiles.fast]\nmodel = \"x\"\n";

    #[test]
    fn codex_merge_adds_our_table_and_keeps_the_rest() {
        let out = merge_codex(Some(CODEX), &engine()).unwrap();
        assert!(out.starts_with(CODEX.trim_end()));
        assert!(out.contains("[mcp_servers.roundtable]\ncommand = \"/Users/a b/Library/Application Support/sh.roundtable.app/engine/roundtable\"\nargs = [\"mcp\", \"serve\"]\n"));
        assert!(out.contains("tool_timeout_sec = 1800"));
        assert!(codex_points_at(Some(&out), &engine()));
        assert!(!codex_points_at(Some(CODEX), &engine()));
        // Again: still one table.
        let again = merge_codex(Some(&out), &engine()).unwrap();
        assert_eq!(again.matches("[mcp_servers.roundtable]").count(), 1);
        assert_eq!(again, out);
    }

    #[test]
    fn codex_merge_replaces_an_old_table_and_its_subtables() {
        let start = "[mcp_servers.roundtable]\ncommand = \"old\"\n\n[mcp_servers.roundtable.env]\nX = \"1\"\n\n[mcp_servers.other]\ncommand = \"o\"\n";
        let out = merge_codex(Some(start), &engine()).unwrap();
        assert!(!out.contains("\"old\"") && !out.contains("X = \"1\""));
        assert!(out.contains("[mcp_servers.other]\ncommand = \"o\""));
        let (gone, changed) = remove_codex(&out);
        assert!(changed);
        assert_eq!(gone, "[mcp_servers.other]\ncommand = \"o\"\n");
        assert!(!remove_codex(&gone).1);
    }

    #[test]
    fn codex_merge_into_a_missing_file_and_leaves_odd_shapes_alone() {
        let out = merge_codex(None, &engine()).unwrap();
        assert!(out.starts_with("[mcp_servers.roundtable]\n"));
        assert!(merge_codex(Some("mcp_servers.roundtable = { command = \"x\" }\n"), &engine()).is_err());
        assert!(merge_codex(Some("[mcp_servers]\nroundtable = { command = \"x\" }\n"), &engine()).is_err());
        // Quoted table names are ours too.
        let q = "[mcp_servers.\"roundtable\"]\ncommand = \"old\"\n";
        assert_eq!(merge_codex(Some(q), &engine()).unwrap().matches("command =").count(), 1);
    }

    #[test]
    fn cursor_merge_uses_the_mcp_servers_shape() {
        let out = merge_cursor(Some(r#"{"mcpServers":{"x":{"url":"https://a"}}}"#), &engine()).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcpServers"]["x"]["url"], "https://a");
        assert_eq!(v["mcpServers"]["roundtable"]["args"], json!(["mcp", "serve"]));
        assert!(merge_cursor(Some("{oops"), &engine()).unwrap_err().contains("Cursor"));
    }

    #[test]
    fn snippets_cover_the_other_clients() {
        let v = snippets(&engine());
        let ids: Vec<_> = v.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, vec!["generic", "claude-code", "codex", "vscode"]);
        assert!(v.as_array().unwrap().iter().all(|s| s["text"].as_str().unwrap().contains("roundtable")));
    }
}
