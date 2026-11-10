//! One small request to check a key works, before it is saved.
//!
//! First a free `GET .../models` call. If the provider has no such route,
//! a one-line chat request with a tiny token limit.

use crate::http::{self, ErrorClass};
use crate::providers::{self, Head, Kind};
use crate::settings::Settings;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct KeyCheck {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<ErrorClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl KeyCheck {
    pub fn ok() -> Self {
        KeyCheck { ok: true, reason: None, message: None }
    }
    pub fn fail(c: ErrorClass) -> Self {
        KeyCheck { ok: false, reason: Some(c), message: Some(c.message().to_string()) }
    }
}

/// The models-list URL next to a head's chat URL.
pub fn models_url(head: &Head) -> String {
    let u = providers::url_for(head);
    if let Some(base) = u.strip_suffix("/chat/completions") {
        format!("{base}/models")
    } else if let Some(base) = u.strip_suffix("/messages") {
        format!("{base}/models")
    } else {
        u
    }
}

pub fn check_key(head: &Head, key: &str, settings: &Settings) -> KeyCheck {
    let t = Duration::from_secs(20);
    let headers = match head.kind {
        Kind::Anthropic => http::anthropic_headers(key),
        Kind::OpenAiCompat => http::bearer(key),
        Kind::CodexCli => return KeyCheck::ok(),
    };
    match http::send("GET", &models_url(head), &headers, None, t) {
        Ok(_) => return KeyCheck::ok(),
        Err(e) if matches!(e.class, ErrorClass::BadKey | ErrorClass::NoCredit | ErrorClass::Network) => {
            return KeyCheck::fail(e.class)
        }
        Err(_) => {}
    }
    let model = providers::model_for(head, settings);
    let (body, url) = match head.kind {
        Kind::Anthropic => (http::anthropic_body(&model, "Say OK.", 8), providers::url_for(head)),
        _ => (http::chat_body(&model, "Say OK.", 16, None), providers::url_for(head)),
    };
    match http::send("POST", &url, &headers, Some(&body), t) {
        Ok(_) => KeyCheck::ok(),
        Err(e) => KeyCheck::fail(e.class),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_urls() {
        assert_eq!(models_url(providers::find("openai").unwrap()), "https://api.openai.com/v1/models");
        assert_eq!(models_url(providers::find("claude").unwrap()), "https://api.anthropic.com/v1/models");
        assert_eq!(
            models_url(providers::find("gemini").unwrap()),
            "https://generativelanguage.googleapis.com/v1beta/openai/models"
        );
    }
}
