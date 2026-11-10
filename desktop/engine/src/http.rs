//! Provider HTTP calls and plain error classes.

use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    BadKey,
    NoCredit,
    RateLimited,
    Timeout,
    Network,
    Other,
}

impl ErrorClass {
    /// One plain line for people.
    pub fn message(self) -> &'static str {
        match self {
            ErrorClass::BadKey => "That key did not work. Check you copied all of it.",
            ErrorClass::NoCredit => "That account has no credit left. Add credit on the provider's site.",
            ErrorClass::RateLimited => "That AI is busy right now. Try again in a minute.",
            ErrorClass::Timeout => "That AI took too long to answer.",
            ErrorClass::Network => "Could not reach that AI. Check your internet connection.",
            ErrorClass::Other => "That AI returned an error.",
        }
    }
}

#[derive(Debug)]
pub struct HttpError {
    pub class: ErrorClass,
    pub status: Option<u16>,
    /// Short tail of the response, never a key.
    pub detail: String,
}

/// Map an HTTP status and body to an error class.
pub fn classify(status: u16, body: &str) -> ErrorClass {
    let b = body.to_ascii_lowercase();
    let credit = ["credit balance", "insufficient_quota", "insufficient balance", "billing", "out of credit", "quota exceeded", "exceeded your current quota", "payment required"]
        .iter()
        .any(|w| b.contains(w));
    match status {
        401 | 403 => ErrorClass::BadKey,
        402 => ErrorClass::NoCredit,
        _ if credit => ErrorClass::NoCredit,
        400 if b.contains("api key") || b.contains("api_key") => ErrorClass::BadKey,
        429 => ErrorClass::RateLimited,
        408 | 504 => ErrorClass::Timeout,
        _ => ErrorClass::Other,
    }
}

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(n);
    chars[start..].iter().collect()
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(timeout)
        .try_proxy_from_env(true)
        .user_agent(concat!("roundtable-engine/", env!("CARGO_PKG_VERSION")))
        .build()
}

pub fn send(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Value, HttpError> {
    let mut req = agent(timeout).request(method, url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    let res = match body {
        Some(b) => req.send_json(b.clone()),
        None => req.call(),
    };
    match res {
        Ok(r) => {
            let text = r.into_string().unwrap_or_default();
            serde_json::from_str(&text).map_err(|_| HttpError {
                class: ErrorClass::Other,
                status: Some(200),
                detail: tail(&text, 160),
            })
        }
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            Err(HttpError { class: classify(code, &text), status: Some(code), detail: tail(&text, 160) })
        }
        Err(ureq::Error::Transport(t)) => {
            let msg = t.to_string();
            let low = msg.to_ascii_lowercase();
            let class = if low.contains("timed out") || low.contains("timeout") {
                ErrorClass::Timeout
            } else {
                ErrorClass::Network
            };
            Err(HttpError { class, status: None, detail: tail(&msg, 160) })
        }
    }
}

pub fn bearer(key: &str) -> Vec<(&'static str, String)> {
    vec![("Authorization", format!("Bearer {key}"))]
}

pub fn anthropic_headers(key: &str) -> Vec<(&'static str, String)> {
    vec![("x-api-key", key.to_string()), ("anthropic-version", "2023-06-01".to_string())]
}

/// Body for an OpenAI-compatible chat call, same fields as `lib/core.sh`.
pub fn chat_body(model: &str, prompt: &str, max_tokens: u32, glm_thinking: Option<bool>) -> Value {
    let mut b = json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "temperature": 0.6,
        "stream": false
    });
    if let Some(on) = glm_thinking {
        b["thinking"] = json!({"type": if on { "enabled" } else { "disabled" }});
    }
    b
}

pub fn anthropic_body(model: &str, prompt: &str, max_tokens: u32) -> Value {
    json!({"model": model, "max_tokens": max_tokens, "messages": [{"role": "user", "content": prompt}]})
}

/// `choices[0].message.content`, trimmed.
pub fn chat_text(v: &Value) -> String {
    v.pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Joined `content[].text`, trimmed.
pub fn anthropic_text(v: &Value) -> String {
    v.get("content")
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|b| b.get("text").and_then(|t| t.as_str())).collect::<String>())
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Drop `<think>...</think>` blocks (MiniMax).
pub fn strip_think(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("<think>") {
        out.push_str(&rest[..i]);
        match rest[i..].find("</think>") {
            Some(j) => rest = &rest[i + j + "</think>".len()..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_errors() {
        assert_eq!(classify(401, ""), ErrorClass::BadKey);
        assert_eq!(classify(403, ""), ErrorClass::BadKey);
        assert_eq!(classify(400, "{\"error\":{\"message\":\"Your credit balance is too low\"}}"), ErrorClass::NoCredit);
        assert_eq!(classify(429, "{\"error\":{\"code\":\"insufficient_quota\"}}"), ErrorClass::NoCredit);
        assert_eq!(classify(429, "slow down"), ErrorClass::RateLimited);
        assert_eq!(classify(400, "API key not valid. Please pass a valid API key."), ErrorClass::BadKey);
        assert_eq!(classify(500, "oops"), ErrorClass::Other);
    }

    #[test]
    fn strips_think_blocks() {
        assert_eq!(strip_think("<think>hmm\nmore</think>\nAnswer"), "Answer");
        assert_eq!(strip_think("A<think>x</think>B"), "AB");
        assert_eq!(strip_think("no tags"), "no tags");
    }

    #[test]
    fn parses_bodies() {
        let v = json!({"choices":[{"message":{"content":"  hi  "}}]});
        assert_eq!(chat_text(&v), "hi");
        let v = json!({"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]});
        assert_eq!(anthropic_text(&v), "ab");
        assert_eq!(chat_body("m", "p", 10, Some(true))["thinking"]["type"], "enabled");
        assert!(chat_body("m", "p", 10, None).get("thinking").is_none());
    }
}
