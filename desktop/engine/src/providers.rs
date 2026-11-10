//! The heads (AI providers) the engine can seat, with the same defaults as `lib/core.sh`.

use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// OpenAI-compatible `/chat/completions`.
    OpenAiCompat,
    /// Anthropic `/v1/messages`.
    Anthropic,
    /// Only through a local command-line tool (see `clis.rs`). No key card in the app.
    Cli,
    /// A model of a provider the person added (OpenRouter, Ollama, ...; see `custom`).
    /// The head id is `<provider>:<model>`.
    Provider,
}

#[derive(Debug, Clone, Copy)]
pub struct Head {
    /// Head id used on the command line: claude, openai, ...
    pub id: &'static str,
    /// Keychain account and plain provider name.
    pub provider: &'static str,
    /// Friendly name for people.
    pub display: &'static str,
    /// Company name, for the transcript label.
    pub company: &'static str,
    pub emoji: &'static str,
    /// Env var the engine reads the key from (first wins).
    pub env_keys: &'static [&'static str],
    pub model_env: &'static str,
    pub default_model: &'static str,
    pub url_env: &'static str,
    pub default_url: &'static str,
    pub max_tokens: u32,
    pub kind: Kind,
    /// Where a person gets a key.
    pub key_page: &'static str,
    /// Shown in the app.
    pub in_app: bool,
}

pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
pub const CHAIR_MODEL_ENV: &str = "ROUNDTABLE_CHAIR_MODEL";
pub const CHAIR_DEFAULT_MODEL: &str = "claude-opus-4-8";

/// Order matches `lib/core.sh` HEADS default.
pub const HEADS: &[Head] = &[
    Head {
        id: "grok",
        provider: "xai",
        display: "Grok",
        company: "xAI",
        emoji: "\u{1f916}",
        env_keys: &["XAI_API_KEY"],
        model_env: "ROUNDTABLE_GROK_MODEL",
        default_model: "grok-4.3",
        url_env: "ROUNDTABLE_GROK_XAI_URL",
        default_url: "https://api.x.ai/v1/chat/completions",
        max_tokens: 6000,
        kind: Kind::OpenAiCompat,
        key_page: "https://console.x.ai",
        in_app: true,
    },
    Head {
        id: "codex",
        provider: "codex",
        display: "Codex",
        company: "OpenAI CLI",
        emoji: "\u{1f9e0}",
        env_keys: &[],
        model_env: "",
        default_model: "",
        url_env: "",
        default_url: "",
        max_tokens: 0,
        kind: Kind::Cli,
        key_page: "",
        in_app: false,
    },
    Head {
        id: "openai",
        provider: "openai",
        display: "ChatGPT",
        company: "OpenAI",
        emoji: "\u{1f9e0}",
        env_keys: &["OPENAI_API_KEY"],
        model_env: "ROUNDTABLE_OPENAI_MODEL",
        default_model: "gpt-5.5",
        url_env: "ROUNDTABLE_OPENAI_URL",
        default_url: "https://api.openai.com/v1/chat/completions",
        max_tokens: 8000,
        kind: Kind::OpenAiCompat,
        key_page: "https://platform.openai.com/api-keys",
        in_app: true,
    },
    Head {
        id: "glm",
        provider: "zai",
        display: "GLM",
        company: "Z.AI",
        emoji: "\u{1f7e3}",
        env_keys: &["ZAI_API_KEY"],
        model_env: "ROUNDTABLE_GLM_MODEL",
        default_model: "glm-5.2",
        url_env: "ROUNDTABLE_GLM_URL",
        default_url: "https://api.z.ai/api/coding/paas/v4/chat/completions",
        max_tokens: 6000,
        kind: Kind::OpenAiCompat,
        key_page: "https://z.ai/manage-apikey/apikey-list",
        in_app: true,
    },
    Head {
        id: "minimax",
        provider: "minimax",
        display: "MiniMax",
        company: "MiniMax",
        emoji: "\u{1f7e0}",
        env_keys: &["MINIMAX_API_KEY"],
        model_env: "ROUNDTABLE_MINIMAX_MODEL",
        default_model: "MiniMax-M3",
        url_env: "ROUNDTABLE_MINIMAX_URL",
        default_url: "https://api.minimax.io/v1/chat/completions",
        max_tokens: 8000,
        kind: Kind::OpenAiCompat,
        key_page: "https://platform.minimax.io/user-center/basic-information/interface-key",
        in_app: true,
    },
    Head {
        id: "claude",
        provider: "anthropic",
        display: "Claude",
        company: "Anthropic",
        emoji: "\u{1f535}",
        env_keys: &["ANTHROPIC_API_KEY"],
        model_env: "ROUNDTABLE_CLAUDE_MODEL",
        default_model: "claude-opus-4-8",
        url_env: "ROUNDTABLE_ANTHROPIC_URL",
        default_url: ANTHROPIC_URL,
        max_tokens: 8000,
        kind: Kind::Anthropic,
        key_page: "https://console.anthropic.com/settings/keys",
        in_app: true,
    },
    Head {
        id: "gemini",
        provider: "google",
        display: "Gemini",
        company: "Google",
        emoji: "\u{2728}",
        env_keys: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        model_env: "ROUNDTABLE_GEMINI_MODEL",
        default_model: "gemini-3.1-pro-preview",
        url_env: "ROUNDTABLE_GEMINI_URL",
        default_url: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        max_tokens: 16000,
        kind: Kind::OpenAiCompat,
        key_page: "https://aistudio.google.com/apikey",
        in_app: true,
    },
    cli_head("qwen", "Qwen", "Qwen Code CLI", "\u{1f7e1}"),
    cli_head("kimi", "Kimi", "Kimi CLI", "\u{1f319}"),
    cli_head("opencode", "OpenCode", "OpenCode CLI", "\u{2b1b}"),
    cli_head("cursor", "Cursor", "Cursor Agent CLI", "\u{1f7e6}"),
];

/// A head that only runs through a local tool, with the tool's own default model.
const fn cli_head(id: &'static str, display: &'static str, company: &'static str, emoji: &'static str) -> Head {
    Head {
        id,
        provider: id,
        display,
        company,
        emoji,
        env_keys: &[],
        model_env: "",
        default_model: "",
        url_env: "",
        default_url: "",
        max_tokens: 0,
        kind: Kind::Cli,
        key_page: "",
        in_app: false,
    }
}


/// The bash engine's default head list.
pub const DEFAULT_HEADS: &str = "grok,codex,openai,glm,minimax,claude,gemini";

/// Find a head by head id or provider name.
/// A `<provider>:<model>` id finds the seat of an added provider (or a keyless preset).
pub fn find(name: &str) -> Option<&'static Head> {
    let n = name.trim().to_ascii_lowercase();
    HEADS.iter().find(|h| h.id == n || h.provider == n).or_else(|| crate::custom::head(name))
}

fn env_nonempty(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Model id: env override, then settings, then the default.
pub fn model_for(head: &Head, settings: &Settings) -> String {
    env_nonempty(head.model_env)
        .or_else(|| settings.models.get(head.id).cloned().filter(|m| !m.trim().is_empty()))
        .unwrap_or_else(|| head.default_model.to_string())
}

pub fn chair_model(settings: &Settings) -> String {
    env_nonempty(CHAIR_MODEL_ENV)
        .or_else(|| settings.models.get("chair").cloned().filter(|m| !m.trim().is_empty()))
        .unwrap_or_else(|| CHAIR_DEFAULT_MODEL.to_string())
}

pub fn url_for(head: &Head) -> String {
    env_nonempty(head.url_env).unwrap_or_else(|| head.default_url.to_string())
}

/// True when this head runs through its local tool rather than a key.
pub fn uses_cli(head: &Head, settings: &Settings) -> bool {
    head.kind == Kind::Cli || settings.cli_heads.iter().any(|h| h == head.id)
}

/// The transcript label, same text as `label_for` in `lib/core.sh`.
/// A head the person set to run through their own tool says so.
pub fn label(head: &Head, settings: &Settings) -> String {
    label_via(head, settings, settings.cli_heads.iter().any(|h| h == head.id))
}

/// The label for a head that did (`tool`) or did not answer through the person's own tool.
pub fn label_via(head: &Head, settings: &Settings, tool: bool) -> String {
    if head.kind == Kind::Provider {
        return crate::custom::label(head);
    }
    let m = model_for(head, settings);
    if head.kind != Kind::Cli && tool {
        if let Some(c) = crate::clis::for_head(head.id) {
            return format!("{} {} ({}, your plan)", head.emoji, head.display, c.name);
        }
    }
    match head.id {
        "grok" => format!("{} Grok ({m}, xAI)", head.emoji),
        "codex" => format!("{} Codex / GPT (OpenAI CLI)", head.emoji),
        "openai" => format!("{} OpenAI ({m})", head.emoji),
        "glm" => format!("{} GLM ({m}, Z.AI)", head.emoji),
        "minimax" => format!("{} MiniMax ({m})", head.emoji),
        "claude" => format!("{} Claude ({m}, Anthropic)", head.emoji),
        "gemini" => format!("{} Gemini ({m}, Google)", head.emoji),
        _ if head.kind == Kind::Cli => format!("{} {} ({})", head.emoji, head.display, head.company),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_head_or_provider() {
        assert_eq!(find("claude").unwrap().provider, "anthropic");
        assert_eq!(find("Anthropic").unwrap().id, "claude");
        assert_eq!(find("google").unwrap().id, "gemini");
        assert!(find("bogus").is_none());
    }

    #[test]
    fn six_heads_in_app() {
        let ids: Vec<_> = HEADS.iter().filter(|h| h.in_app).map(|h| h.id).collect();
        assert_eq!(ids.len(), 6);
        assert!(!ids.contains(&"codex"));
    }

    #[test]
    fn cli_heads_and_labels() {
        let mut s = Settings::default();
        assert_eq!(label(find("qwen").unwrap(), &s), "\u{1f7e1} Qwen (Qwen Code CLI)");
        assert!(uses_cli(find("codex").unwrap(), &s));
        assert!(!uses_cli(find("claude").unwrap(), &s));
        s.cli_heads = vec!["claude".into()];
        assert!(uses_cli(find("claude").unwrap(), &s));
        assert_eq!(label(find("claude").unwrap(), &s), "\u{1f535} Claude (Claude Code, your plan)");
        assert_eq!(label(find("grok").unwrap(), &Settings::default()), "\u{1f916} Grok (grok-4.3, xAI)");
    }

    #[test]
    fn settings_model_override() {
        let mut s = Settings::default();
        s.models.insert("glm".into(), "glm-test".into());
        assert_eq!(model_for(find("glm").unwrap(), &s), "glm-test");
        assert_eq!(model_for(find("minimax").unwrap(), &s), "MiniMax-M3");
    }
}
