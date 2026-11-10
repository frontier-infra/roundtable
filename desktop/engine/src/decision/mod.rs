//! The decision model: a System One model rates the table's verdict.
//!
//! After the final round, the engine turns the verdict into typed options (what the
//! seats argued, plus "need more information") and asks a decision model for a
//! calibrated probability per option and a confidence. The result is one section
//! of the transcript (so the verdict, history and the Markdown export all carry it)
//! and one `decision` event for the app.
//!
//! Providers (all speak the System One wire, `POST /v1/systemone`):
//! - `jev`: TypeSafe's hosted Jev. Needs the person's key.
//! - `jebadiah`: Jebadiah on this computer (`jeb serve`, AINode) or at a URL.
//! - `systemone`: any other System One model at a URL with a model id (LINK21 slot).
//!
//! The hook the verdict step calls is [`after_verdict`]. See `HOOK.md` beside this file.

pub mod cli;
pub mod detect;
pub mod options;
pub mod provider;

use crate::keys::Keys;
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub use options::{OptionSource, TypedOption, MORE_INFO};
pub use provider::{Answer, DecisionModel};

/// `settings.decision`. `provider: None` means off (the default).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct DecisionSettings {
    /// "jev", "jebadiah" or "systemone". None or "off" turns it off.
    pub provider: Option<String>,
    /// Base URL for jebadiah or systemone, for example `http://127.0.0.1:8100`.
    /// Empty for jebadiah means look on this computer.
    pub url: Option<String>,
    /// Model id. Empty means the provider's default (Jev: `jev-latest`; Jebadiah: what the server lists).
    pub model: Option<String>,
    /// Display name for the systemone slot, for example "LINK21".
    pub name: Option<String>,
}

impl DecisionSettings {
    /// The chosen provider id, or None when off.
    pub fn active(&self) -> Option<&str> {
        self.provider.as_deref().map(str::trim).filter(|p| !p.is_empty() && *p != "off")
    }
}

/// Provider ids, in the order the app lists them.
pub const PROVIDERS: &[&str] = &["jev", "jebadiah", "systemone"];

/// Map a flag value to a provider id. "off" and "none" come back as Some("off").
pub fn parse_provider(v: &str) -> Option<&'static str> {
    match v.trim().to_ascii_lowercase().as_str() {
        "jev" | "typesafe" => Some("jev"),
        "jebadiah" | "jeb" => Some("jebadiah"),
        "systemone" | "system-one" | "other" | "link21" => Some("systemone"),
        "off" | "none" | "" => Some("off"),
        _ => None,
    }
}

/// Apply a `--decision` flag or MCP argument on top of the saved settings.
pub fn with_override(mut settings: Settings, flag: Option<&str>) -> Result<Settings, String> {
    let Some(v) = flag else { return Ok(settings) };
    match parse_provider(v) {
        Some("off") => settings.decision.provider = None,
        Some(p) => {
            if settings.decision.active() != Some(p) {
                // The saved address and model belong to the saved provider.
                settings.decision.url = None;
                settings.decision.model = None;
            }
            if v.eq_ignore_ascii_case("link21") && settings.decision.name.is_none() {
                settings.decision.name = Some("LINK21".into());
            }
            settings.decision.provider = Some(p.into());
        }
        None => return Err(format!("Unknown decision model '{v}'. Use jev, jebadiah, systemone or off.")),
    }
    Ok(settings)
}

/// What the verdict step hands the hook: the question and the final round.
#[derive(Debug, Clone)]
pub struct Verdict<'a> {
    pub question: &'a str,
    pub context: Option<&'a str>,
    /// (seat label, final position) for every seat, in table order. Skipped seats carry the sentinel text.
    pub positions: &'a [(String, String)],
    /// The chair's last verdict, when the table talked it through.
    pub chair: Option<&'a str>,
    /// Some(true) when the chair declared consensus; None in advisory mode.
    pub consensus: Option<bool>,
}

/// The decision model's reading of the verdict.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Report {
    /// Display name: "Jev", "Jebadiah", "LINK21".
    pub provider: String,
    pub model: String,
    /// Where it was asked (no key, ever).
    pub endpoint: String,
    pub options: Vec<TypedOption>,
    /// One per option in `options`, same order; sums to 1.
    pub probabilities: Vec<f64>,
    pub confidence: Option<f64>,
    /// Key of the highest-rated option.
    pub top: String,
    /// The option the table settled on, when the chair named one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_pick: Option<String>,
    pub disagrees: bool,
    /// The one line the verdict shows.
    pub line: String,
    /// The plain disagreement sentence, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disagreement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// When the decision model was on but could not answer.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Unavailable {
    pub provider: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum Outcome {
    Rated(Report),
    Unavailable(Unavailable),
}

impl Outcome {
    /// The transcript section: a heading, the line, the options, any disagreement.
    pub fn markdown(&self) -> String {
        let mut s = String::from("### Decision model\n\n");
        match self {
            Outcome::Rated(r) => {
                s.push_str(&r.line);
                s.push_str("\n\n");
                for (o, p) in r.options.iter().zip(&r.probabilities) {
                    s.push_str(&format!("- **{}** ({}): {}\n", o.label(), fmt_p(*p), o.text));
                }
                s.push('\n');
                if let Some(d) = &r.disagreement {
                    s.push_str(&format!("**{d}**\n\n"));
                }
                s.push_str(&format!("_Asked {} ({}). Probabilities are the decision model's, not the table's._\n", r.provider, r.model));
            }
            Outcome::Unavailable(u) => {
                s.push_str(&format!("Decision model: {} did not answer. {}\n", u.provider, u.message));
            }
        }
        s
    }
}

pub fn fmt_p(p: f64) -> String {
    format!("{:.2}", p)
}

/// The verdict line: "Decision model: Jev rates option A 0.62, B 0.31, more information 0.07".
pub fn verdict_line(name: &str, options: &[TypedOption], probs: &[f64], confidence: Option<f64>) -> String {
    let parts: Vec<String> = options
        .iter()
        .zip(probs)
        .enumerate()
        .map(|(i, (o, p))| {
            let label = if o.key == MORE_INFO { "more information".to_string() } else if i == 0 { format!("option {}", o.key) } else { o.key.clone() };
            format!("{label} {}", fmt_p(*p))
        })
        .collect();
    let mut line = format!("Decision model: {name} rates {}", parts.join(", "));
    if let Some(c) = confidence {
        line.push_str(&format!(". Confidence {}", fmt_p(c)));
    }
    line.push('.');
    line
}

/// Build the report from the model's answer. Pure, so tests cover it without a network.
pub fn report(model: &dyn DecisionModel, options: &[TypedOption], table_pick: Option<&str>, consensus: Option<bool>, ans: &Answer) -> Report {
    let raw: Vec<f64> = options.iter().map(|o| ans.probabilities.iter().find(|(k, _)| *k == o.key).map(|(_, p)| p.max(0.0)).unwrap_or(0.0)).collect();
    let sum: f64 = raw.iter().sum();
    let probs: Vec<f64> = if sum > 0.0 { raw.iter().map(|p| p / sum).collect() } else { raw };
    let top_i = probs.iter().enumerate().fold(0, |b, (i, p)| if *p > probs[b] { i } else { b });
    let top = options[top_i].key.clone();
    let name = model.name().to_string();
    let table_pick = table_pick.filter(|k| options.iter().any(|o| o.key == *k)).map(str::to_string);
    let who = if consensus == Some(true) { "the table's consensus" } else { "the table's lead answer" };
    let disagreement = match &table_pick {
        Some(t) if *t != top => Some(if top == MORE_INFO {
            format!(
                "The decision model disagrees with {who}: the table chose option {t}, but {name} says more information is needed first ({}).",
                fmt_p(probs[top_i])
            )
        } else {
            format!("The decision model disagrees with {who}: the table chose option {t}, but {name} rates option {top} highest ({}).", fmt_p(probs[top_i]))
        }),
        _ => None,
    };
    Report {
        provider: name.clone(),
        model: ans.model.clone(),
        endpoint: model.endpoint().to_string(),
        options: options.to_vec(),
        line: verdict_line(&name, options, &probs, ans.confidence),
        probabilities: probs,
        confidence: ans.confidence,
        top,
        disagrees: disagreement.is_some(),
        disagreement,
        table_pick,
        latency_ms: ans.latency_ms,
    }
}

/// Core of the hook with every dependency passed in (tests use fakes).
pub fn decide(v: &Verdict, model: &dyn DecisionModel, source: &dyn OptionSource) -> Outcome {
    let set = source.options(v);
    if set.options.len() < 2 {
        return Outcome::Unavailable(Unavailable { provider: model.name().into(), message: "The table gave no options to rate.".into() });
    }
    let state = provider::state_for(v, &set.options);
    match model.choose(&state, &provider::instructions(), &set.options) {
        Ok(ans) => Outcome::Rated(report(model, &set.options, set.table_pick.as_deref(), v.consensus, &ans)),
        Err(e) => Outcome::Unavailable(Unavailable { provider: model.name().into(), message: e }),
    }
}

/// THE HOOK. The verdict step calls this once, after the final round.
/// Returns None when the decision model is off. Never panics, never blocks past
/// `timeout` per call, and never puts a key in its output.
pub fn after_verdict(v: &Verdict, keys: &Keys, settings: &Settings, timeout: Duration) -> Option<Outcome> {
    let id = settings.decision.active()?;
    let model = match provider::resolve(id, &settings.decision, keys) {
        Ok(m) => m,
        Err(u) => return Some(Outcome::Unavailable(u)),
    };
    let source = options::ChairOptions { keys, settings, timeout };
    Some(decide(v, model.as_ref(), &source))
}

#[cfg(test)]
mod tests;
