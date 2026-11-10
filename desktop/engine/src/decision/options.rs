//! Turn the verdict into typed options.
//!
//! The chair (Claude, by the same route as the council) lists the distinct options
//! the seats argued and which one the table settled on. If the chair cannot be
//! asked, each answered seat's opening line becomes an option.

use super::Verdict;
use crate::council::{self, Outcome, Route};
use crate::keys::Keys;
use crate::providers;
use crate::settings::Settings;
use serde::Serialize;
use std::time::Duration;

/// Key of the option every set ends with.
pub const MORE_INFO: &str = "more_information";
const MORE_INFO_TEXT: &str = "Need more information: the arguments do not yet support choosing any option.";
/// At most this many argued options (local runtimes cap a question at 20).
pub const MAX_OPTIONS: usize = 6;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TypedOption {
    /// "A", "B", ... or `more_information`.
    pub key: String,
    pub text: String,
}

impl TypedOption {
    pub fn label(&self) -> String {
        if self.key == MORE_INFO {
            "More information".into()
        } else {
            format!("Option {}", self.key)
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OptionSet {
    /// Argued options, then `more_information` last.
    pub options: Vec<TypedOption>,
    /// The key the table settled on, when known.
    pub table_pick: Option<String>,
}

pub trait OptionSource {
    fn options(&self, v: &Verdict) -> OptionSet;
}

/// Letter keys plus the closing "more information" option.
pub fn build(texts: &[String], pick_index: Option<usize>) -> OptionSet {
    let mut seen: Vec<String> = Vec::new();
    let mut map: Vec<usize> = Vec::new();
    for t in texts {
        let t = t.trim();
        if t.is_empty() {
            map.push(usize::MAX);
            continue;
        }
        match seen.iter().position(|s| s.eq_ignore_ascii_case(t)) {
            Some(i) => map.push(i),
            None if seen.len() < MAX_OPTIONS => {
                seen.push(t.to_string());
                map.push(seen.len() - 1);
            }
            None => map.push(usize::MAX),
        }
    }
    let mut options: Vec<TypedOption> =
        seen.iter().enumerate().map(|(i, t)| TypedOption { key: ((b'A' + i as u8) as char).to_string(), text: t.clone() }).collect();
    let table_pick = pick_index.and_then(|i| map.get(i).copied()).filter(|i| *i != usize::MAX).map(|i| options[i].key.clone());
    if !options.is_empty() {
        options.push(TypedOption { key: MORE_INFO.into(), text: MORE_INFO_TEXT.into() });
    }
    OptionSet { options, table_pick }
}

/// The chair prompt that lists the options. Asks for JSON only.
pub fn extract_prompt(v: &Verdict) -> String {
    let mut p = format!(
        "You are the neutral CHAIR of an AI council. Turn the council's final positions into a short list of typed options for a decision model.\n\nQUESTION:\n{}\n\n--- FINAL POSITIONS ---\n\n",
        v.question
    );
    for (label, body) in v.positions {
        p.push_str(&format!("### {label}\n{}\n\n", clip(body, 3000)));
    }
    if let Some(c) = v.chair {
        p.push_str(&format!("--- CHAIR VERDICT ---\n{}\n\n", clip(c, 3000)));
    }
    p.push_str(&format!(
        "--- YOUR JOB ---\nList the distinct answers the councilors argued for, 2 to {MAX_OPTIONS} of them, each one self-contained sentence of at most 30 words. If they all agree, list the agreed answer and the strongest alternative anyone raised. Then say which one the table settled on (its index from 0), or null if it did not settle.\nReply with JSON only, no prose, exactly this shape:\n{{\"options\": [\"...\", \"...\"], \"table_pick\": 0}}\n"
    ));
    p
}

/// Read the chair's JSON reply. None when it is not usable.
pub fn parse_reply(text: &str) -> Option<OptionSet> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let v: serde_json::Value = serde_json::from_str(text.get(start..=end)?).ok()?;
    let texts: Vec<String> = v.get("options")?.as_array()?.iter().filter_map(|o| o.as_str().map(|s| clip(s.trim(), 400))).collect();
    let pick = v.get("table_pick").and_then(|p| p.as_u64()).map(|p| p as usize);
    let set = build(&texts, pick);
    (set.options.len() >= 3).then_some(set)
}

/// Fallback: one option per answered seat, its opening line.
pub fn from_positions(v: &Verdict) -> OptionSet {
    let texts: Vec<String> = v.positions.iter().filter(|(_, b)| !is_sentinel(b)).map(|(_, b)| opening(b)).collect();
    build(&texts, None)
}

fn is_sentinel(body: &str) -> bool {
    let t = body.trim();
    t.is_empty() || (t.starts_with("_(") && t.ends_with(")_"))
}

/// First meaningful line, without markdown marks or a leading "POSITION:".
pub fn opening(body: &str) -> String {
    let line = body
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '*', '-', '>', ' ']).trim())
        .find(|l| !l.is_empty() && !l.eq_ignore_ascii_case("position:"))
        .unwrap_or("");
    let line = line.strip_prefix("POSITION:").or_else(|| line.strip_prefix("Position:")).unwrap_or(line);
    clip(line.replace("**", "").replace('`', "").trim(), 240)
}

pub fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n).collect();
        t.push_str("...");
        t
    }
}

/// The real source: ask the chair, fall back to the seats' opening lines.
pub struct ChairOptions<'a> {
    pub keys: &'a Keys<'a>,
    pub settings: &'a Settings,
    pub timeout: Duration,
}

impl OptionSource for ChairOptions<'_> {
    fn options(&self, v: &Verdict) -> OptionSet {
        let claude = providers::find("claude").expect("claude head");
        let r = council::ask_head(claude, &extract_prompt(v), false, self.timeout, self.keys, self.settings, Route::Auto, Some(&providers::chair_model(self.settings)));
        if let Outcome::Answered(t) = r.outcome {
            if let Some(set) = parse_reply(&t) {
                return set;
            }
        }
        from_positions(v)
    }
}
