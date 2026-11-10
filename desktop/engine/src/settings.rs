//! `settings.json` in the data folder. Never holds a key.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Head ids that sit at the table by default. Empty means every head with a key.
    pub default_heads: Vec<String>,
    /// Model id per head id (and "chair").
    pub models: BTreeMap<String, String>,
    /// Seconds per AI. None means 300, or 360 with web search.
    pub timeout_secs: Option<u64>,
    /// Keep past roundtables on this computer.
    pub history: bool,
    pub auto_update: bool,
    /// The person finished the five setup screens.
    pub onboarded: bool,
    /// "Use Roundtable in Terminal too".
    pub terminal_command: bool,
    /// Head ids that run through the person's own installed tool (Claude Code, Codex, ...).
    pub cli_heads: Vec<String>,
    /// The person has ticked or unticked a tool, so detection stops choosing for them.
    pub clis_chosen: bool,
    /// Show the Frontier labs persona pack. The single switch, with the `lab-pack` build feature.
    pub show_lab_pack: bool,
    /// Seats for a run with no `--personas`: a preset, pack or persona list. Empty is Generic.
    pub personas: String,
    /// The question coach opens before a question goes to the table.
    pub coach: bool,
    /// Round 1 is blind: seats answer without seeing each other.
    pub blind_first_round: bool,
    /// The verdict shape the ask box starts with: decision, pros-cons, plan or vote.
    pub verdict_shape: String,
    /// The decision model that rates the verdict. Off unless a provider is chosen.
    pub decision: crate::decision::DecisionSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_heads: Vec::new(),
            models: BTreeMap::new(),
            timeout_secs: None,
            history: true,
            auto_update: true,
            onboarded: false,
            terminal_command: false,
            cli_heads: Vec::new(),
            clis_chosen: false,
            show_lab_pack: true,
            personas: String::new(),
            coach: true,
            blind_first_round: true,
            verdict_shape: "decision".into(),
            decision: Default::default(),
        }
    }
}

impl Settings {
    pub fn load_from(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn load() -> Settings {
        Self::load_from(&crate::paths::settings_file())
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&crate::paths::settings_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_bad_file_gives_defaults() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        assert_eq!(Settings::load_from(&p), Settings::default());
        std::fs::write(&p, "{not json").unwrap();
        assert!(Settings::load_from(&p).history);
    }

    #[test]
    fn round_trips() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        let mut s = Settings::default();
        s.default_heads = vec!["claude".into(), "gemini".into()];
        s.history = false;
        s.save_to(&p).unwrap();
        assert_eq!(Settings::load_from(&p), s);
    }
}
