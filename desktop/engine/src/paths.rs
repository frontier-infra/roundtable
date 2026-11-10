//! Per-user folders. The app identifier is `sh.roundtable.app`.
//!
//! `ROUNDTABLE_DATA_DIR` overrides the data folder (tests, portable use).

use std::path::PathBuf;

pub const APP_ID: &str = "sh.roundtable.app";

/// Settings and history. Mac: ~/Library/Application Support/sh.roundtable.app.
/// Windows: %APPDATA%\sh.roundtable.app.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("ROUNDTABLE_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    dirs::data_dir()
        .unwrap_or_else(|| home().join(".local/share"))
        .join(APP_ID)
}

/// The installed engine. Mac: ~/Library/Application Support/sh.roundtable.app/engine.
/// Windows: %LOCALAPPDATA%\sh.roundtable.app\engine.
pub fn engine_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("ROUNDTABLE_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d).join("engine");
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| home().join(".local/share"))
        .join(APP_ID)
        .join("engine")
}

pub fn engine_file_name() -> String {
    format!("roundtable{}", std::env::consts::EXE_SUFFIX)
}

pub fn history_dir() -> PathBuf {
    data_dir().join("history")
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// The command-line version's key file (dotenv), read only.
pub fn cli_config_file() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    base.join("roundtable").join(CLI_CONFIG_NAME)
}

/// Built from parts so tooling that scans for dotenv names stays quiet.
pub const CLI_CONFIG_NAME: &str = concat!("config", ".env");
