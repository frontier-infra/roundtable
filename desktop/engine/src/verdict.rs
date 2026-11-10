//! Verdict shapes: the chair's last word on a run. Same prompts as `lib/core.sh`.
//!
//! A shape is a decision with confidence, pros and cons, an action plan, or a vote.
//! Every shape ends by naming the minority view and who held it.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    Decision,
    ProsCons,
    Plan,
    Vote,
}

pub const SHAPES: &[Shape] = &[Shape::Decision, Shape::ProsCons, Shape::Plan, Shape::Vote];

impl Shape {
    /// `decision`, `pros-cons`, `plan` or `vote`. `none` and empty mean no verdict.
    pub fn parse(s: &str) -> Result<Option<Shape>, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "none" | "off" => Ok(None),
            "decision" => Ok(Some(Shape::Decision)),
            "pros-cons" | "pros_cons" | "proscons" | "pros" => Ok(Some(Shape::ProsCons)),
            "plan" | "action-plan" | "action" => Ok(Some(Shape::Plan)),
            "vote" => Ok(Some(Shape::Vote)),
            other => Err(format!("Unknown verdict '{other}'. Use decision, pros-cons, plan or vote.")),
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Shape::Decision => "decision",
            Shape::ProsCons => "pros-cons",
            Shape::Plan => "plan",
            Shape::Vote => "vote",
        }
    }
    /// The words people see.
    pub fn title(self) -> &'static str {
        match self {
            Shape::Decision => "Decision",
            Shape::ProsCons => "Pros and cons",
            Shape::Plan => "Action plan",
            Shape::Vote => "Vote",
        }
    }
    fn job(self) -> &'static str {
        match self {
            Shape::Decision => "Shape: DECISION. Line 1: \"DECISION: <the answer in one sentence>\". Line 2: \"CONFIDENCE: <high, medium or low> (<0 to 100>%)\". Then **WHY** (at most four bullets).",
            Shape::ProsCons => "Shape: PROS AND CONS. Line 1: \"OPTION: <the leading option>\". Then **PROS** (bullets) and **CONS** (bullets), then \"RECOMMENDATION: <one sentence>\".",
            Shape::Plan => "Shape: ACTION PLAN. Line 1: \"GOAL: <one sentence>\". Then **STEPS** as a numbered list, each step with who and by when, then \"FIRST STEP TODAY: <one sentence>\".",
            Shape::Vote => "Shape: VOTE. Give **VOTES**: one bullet per seat, \"<seat name>: <the option it backs>\". Then \"RESULT: <option> wins <n> to <m>\", or \"RESULT: tie\".",
        }
    }
}

/// The chair's prompt for the verdict. `table` is (label, final answer) per seat.
pub fn prompt(shape: Shape, question: &str, brief: Option<&str>, attachments: Option<&str>, table: &[(String, String)]) -> String {
    let mut p = format!("You are the neutral CHAIR of an AI council. Write the final verdict on this question:\n\n{}\n\n", question.trim());
    if let Some(b) = brief.filter(|b| !b.trim().is_empty()) {
        p.push_str(&format!("THE BRIEF:\n{}\n\n", b.trim()));
    }
    // The chair reads the same files and links as every seat, or it doubts what they quote.
    if let Some(m) = attachments.filter(|m| !m.trim().is_empty()) {
        p.push_str(&format!("THE MATERIAL EVERY SEAT READ:\n{}\n\n", m.trim()));
    }
    p.push_str("--- FINAL POSITIONS ---\n\n");
    for (label, body) in table {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    p.push_str(&format!(
        "--- YOUR JOB ---\n{}\nThen end with exactly these two lines:\nMINORITY VIEW: <the strongest view that differs from the verdict, in one sentence, or \"None\">\nHELD BY: <the seats that held it, by their names above, or \"Nobody\">\n",
        shape.job()
    ));
    p
}

pub const UNAVAILABLE: &str = "_(verdict unavailable: the chair did not answer)_";

/// What the app needs from a verdict besides its text.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Parts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_by: Option<String>,
    /// Decision only: high, medium or low.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
}

fn after(line: &str, key: &str) -> Option<String> {
    let t = line.trim().replace("**", "");
    let t = t.trim().trim_start_matches(['-', '*']).trim();
    (t.len() > key.len() && t[..key.len()].eq_ignore_ascii_case(key)).then(|| t[key.len()..].trim().to_string())
}

/// The minority view, who held it, and the confidence, from the chair's text.
pub fn parts(text: &str) -> Parts {
    let mut p = Parts::default();
    for line in text.lines() {
        if let Some(v) = after(line, "MINORITY VIEW:") {
            p.minority = Some(v).filter(|v| !v.is_empty() && !v.trim_matches(['"', '.']).eq_ignore_ascii_case("none"));
        } else if let Some(v) = after(line, "HELD BY:") {
            p.held_by = Some(v).filter(|v| !v.is_empty() && !v.trim_matches(['"', '.']).eq_ignore_ascii_case("nobody"));
        } else if let Some(v) = after(line, "CONFIDENCE:") {
            let low = v.to_ascii_lowercase();
            p.confidence = ["high", "medium", "low"].iter().find(|w| low.starts_with(**w)).map(|w| w.to_string());
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_parse() {
        assert_eq!(Shape::parse("decision").unwrap(), Some(Shape::Decision));
        assert_eq!(Shape::parse("Pros-Cons").unwrap(), Some(Shape::ProsCons));
        assert_eq!(Shape::parse("none").unwrap(), None);
        assert!(Shape::parse("poll").is_err());
        for s in SHAPES {
            assert_eq!(Shape::parse(s.id()).unwrap(), Some(*s));
        }
    }

    #[test]
    fn prompt_names_the_minority_lines() {
        let p = prompt(Shape::Vote, "Q", Some("- **Goal:** g"), None, &[("A".into(), "x".into())]);
        assert!(p.contains("THE BRIEF:\n- **Goal:** g"));
        assert!(!p.contains("THE MATERIAL"));
        let m = prompt(Shape::Decision, "Q", None, Some("### notes.txt\nCode word: HERON-42.\n"), &[("A".into(), "x".into())]);
        assert!(m.contains("THE MATERIAL EVERY SEAT READ:\n### notes.txt\nCode word: HERON-42.\n\n--- FINAL POSITIONS ---"));
        assert!(p.contains("MINORITY VIEW:") && p.contains("HELD BY:"));
        assert!(!p.contains('\u{2014}'));
    }

    #[test]
    fn parts_read_back() {
        let t = "DECISION: Start the garden.\nCONFIDENCE: Medium (65%)\n**WHY**\n- x\nMINORITY VIEW: Bake bread first.\nHELD BY: Gemini";
        assert_eq!(parts(t), Parts { minority: Some("Bake bread first.".into()), held_by: Some("Gemini".into()), confidence: Some("medium".into()) });
        let none = parts("RESULT: tie\nMINORITY VIEW: None\nHELD BY: Nobody");
        assert_eq!(none, Parts::default());
    }
}
