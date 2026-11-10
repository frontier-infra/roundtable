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
    /// What the shape adds after the decision core.
    fn job(self) -> &'static str {
        match self {
            Shape::Decision => "Then **WHY** (at most four bullets) and **RISKS** (at most three bullets: what could hurt this person).",
            Shape::ProsCons => "Then **PROS** (bullets) and **CONS** (bullets) of the pick.",
            Shape::Plan => "Then **PLAN**: the rest of the steps as a numbered list, each step with who and by when.",
            Shape::Vote => "Then **VOTES**: one bullet per seat, \"<seat name>: <the option it backs>\". Then \"RESULT: <option> wins <n> to <m>\", or \"RESULT: tie\".",
        }
    }
}

/// Every verdict leads with one decision, whatever its shape: the pick, how sure,
/// what would change it, and three ordered next steps.
pub const CORE: &str = "Lead with one decision. Start with exactly these lines:\nDECISION: <the pick, in one sentence>\nCONFIDENCE: <high, medium or low> (<0 to 100>%)\nWOULD CHANGE IT: <the fact or event that would change the pick, in one sentence>\nNEXT STEPS:\n1. <the first concrete step, one the person can start today>\n2. <the second step>\n3. <the third step>\n";

/// When the question lacks key facts, the verdict asks first, then gives the pick that
/// holds under the likely answers.
pub const CLARIFY: &str = "Key facts are missing (the OPEN QUESTIONS above), so open with the questions, not the pick. Start with exactly this line, then two or three numbered questions (sharpen the open questions if that helps):\nQUESTIONS FIRST:\n1. <question>\n2. <question>\nThen give the decision lines below. DECISION names the pick that holds under the most likely answers and says how the answers would change it (\"If ..., then ...\").\n";

/// The chair's prompt for the verdict. `table` is (label, final answer) per seat;
/// `attachments` the material every seat read; `open` the chair's open questions when the question lacks key facts.
pub fn prompt(shape: Shape, question: &str, brief: Option<&str>, attachments: Option<&str>, open: &[String], table: &[(String, String)]) -> String {
    let mut p = format!("You are the neutral CHAIR of an AI council. Write the final verdict on this question:\n\n{}\n\n", question.trim());
    if let Some(b) = brief.filter(|b| !b.trim().is_empty()) {
        p.push_str(&format!("THE BRIEF:\n{}\n\n", b.trim()));
    }
    // The chair reads the same files and links as every seat, or it doubts what they quote.
    if let Some(m) = attachments.filter(|m| !m.trim().is_empty()) {
        p.push_str(&format!("THE MATERIAL EVERY SEAT READ:\n{}\n\n", m.trim()));
    }
    if !open.is_empty() {
        p.push_str("OPEN QUESTIONS:\n");
        for q in open {
            p.push_str(&format!("- {q}\n"));
        }
        p.push('\n');
    }
    p.push_str("--- FINAL POSITIONS ---\n\n");
    for (label, body) in table {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    p.push_str("--- YOUR JOB ---\n");
    if !open.is_empty() {
        p.push_str(CLARIFY);
    }
    p.push_str(&format!(
        "{CORE}{}\nUse the seats' own numbers and add no new facts. Keep it under 300 words: the person reads this first, and the seat answers sit folded under it.\nThen end with exactly these two lines:\nMINORITY VIEW: <the strongest view that differs from the verdict, in one sentence, or \"None\">\nHELD BY: <the seats that held it, by their names above, or \"Nobody\">\n",
        shape.job()
    ));
    p
}

/// The verdict a run gets: the shape asked for, or a decision on any table of two seats
/// or more when none was asked for. `none` (or `off`) turns it off.
pub fn choose(given: Option<&str>, table: usize) -> Result<Option<Shape>, String> {
    match given {
        Some(g) => Shape::parse(g),
        None if table >= 2 => Ok(Some(Shape::Decision)),
        None => Ok(None),
    }
}

pub const UNAVAILABLE: &str = "_(verdict unavailable: the chair did not answer)_";

/// What the app needs from a verdict besides its text.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Parts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_by: Option<String>,
    /// High, medium or low.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// The pick, in one sentence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    /// What would change the pick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub would_change: Option<String>,
    /// The ordered next steps.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub next_steps: Vec<String>,
    /// The questions the chair opens with when key facts are missing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<String>,
}

fn after(line: &str, key: &str) -> Option<String> {
    let t = line.trim().replace("**", "");
    let t = t.trim().trim_start_matches(['-', '*']).trim();
    (t.len() > key.len() && t[..key.len()].eq_ignore_ascii_case(key)).then(|| t[key.len()..].trim().to_string())
}

/// A numbered or bulleted item: "1. x", "2) x", "- x". None for any other line.
fn item(line: &str) -> Option<String> {
    let t = line.trim().replace("**", "");
    let t = t.trim();
    if let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
        return Some(rest.trim().to_string()).filter(|r| !r.is_empty());
    }
    let (n, rest) = t.split_once(['.', ')'])?;
    (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).then(|| rest.trim().to_string()).filter(|r| !r.is_empty())
}

/// The decision, confidence, what would change it, the next steps, the open questions,
/// the minority view and who held it, from the chair's text.
pub fn parts(text: &str) -> Parts {
    let mut p = Parts::default();
    // Which list the numbered lines belong to: 1 next steps, 2 questions.
    let mut list = 0;
    for line in text.lines() {
        if list > 0 {
            let v = if list == 1 { &mut p.next_steps } else { &mut p.questions };
            if let Some(x) = item(line) {
                // A numbered line starts an item; an indented line or a bullet under one adds to it.
                let numbered = line.trim().replace("**", "").trim().starts_with(|c: char| c.is_ascii_digit());
                let indented = line.starts_with(' ') || line.starts_with('\t');
                match v.last_mut() {
                    Some(last) if indented || !numbered => {
                        last.push_str("; ");
                        last.push_str(&x);
                    }
                    _ => v.push(x),
                }
                continue;
            }
            if line.trim().is_empty() {
                continue;
            }
            list = 0;
        }
        if let Some(v) = after(line, "MINORITY VIEW:") {
            p.minority = Some(v).filter(|v| !v.is_empty() && !v.trim_matches(['"', '.']).eq_ignore_ascii_case("none"));
        } else if let Some(v) = after(line, "HELD BY:") {
            p.held_by = Some(v).filter(|v| !v.is_empty() && !v.trim_matches(['"', '.']).eq_ignore_ascii_case("nobody"));
        } else if let Some(v) = after(line, "CONFIDENCE:") {
            let low = v.to_ascii_lowercase();
            p.confidence = ["high", "medium", "low"].iter().find(|w| low.starts_with(**w)).map(|w| w.to_string());
        } else if let Some(v) = after(line, "DECISION:") {
            p.decision = Some(v).filter(|v| !v.is_empty());
        } else if let Some(v) = after(line, "WOULD CHANGE IT:") {
            p.would_change = Some(v).filter(|v| !v.is_empty());
        } else if is_key(line, "NEXT STEPS:") {
            list = 1;
        } else if is_key(line, "QUESTIONS FIRST:") {
            list = 2;
        }
    }
    p
}

/// A line that is only this key ("NEXT STEPS:", maybe bold).
fn is_key(line: &str, key: &str) -> bool {
    let t = line.trim().replace("**", "");
    t.trim().eq_ignore_ascii_case(key)
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
        let p = prompt(Shape::Vote, "Q", Some("- **Goal:** g"), None, &[], &[("A".into(), "x".into())]);
        assert!(p.contains("THE BRIEF:\n- **Goal:** g"));
        assert!(!p.contains("THE MATERIAL"));
        let m = prompt(Shape::Decision, "Q", None, Some("### notes.txt\nCode word: HERON-42.\n"), &[], &[("A".into(), "x".into())]);
        assert!(m.contains("THE MATERIAL EVERY SEAT READ:\n### notes.txt\nCode word: HERON-42.\n\n--- FINAL POSITIONS ---"));
        assert!(p.contains("MINORITY VIEW:") && p.contains("HELD BY:"));
        assert!(!p.contains('\u{2014}'));
        for s in SHAPES {
            let p = prompt(*s, "Q", None, None, &[], &[]);
            assert!(p.contains("--- YOUR JOB ---\nLead with one decision.") && p.contains("WOULD CHANGE IT:") && p.contains("NEXT STEPS:\n1."), "every shape keeps the core: {p}");
            assert!(!p.contains("QUESTIONS FIRST"));
        }
        let p = prompt(Shape::Decision, "Q", None, None, &["What is the workload?".into(), "How big?".into()], &[]);
        assert!(p.contains("OPEN QUESTIONS:\n- What is the workload?\n- How big?\n\n--- FINAL POSITIONS"));
        assert!(p.contains("--- YOUR JOB ---\nKey facts are missing") && p.find("QUESTIONS FIRST").unwrap() < p.find("DECISION:").unwrap());
    }

    #[test]
    fn a_table_gets_a_decision_by_default() {
        assert_eq!(choose(None, 3).unwrap(), Some(Shape::Decision));
        assert_eq!(choose(None, 1).unwrap(), None, "one AI answers on its own");
        assert_eq!(choose(Some("none"), 3).unwrap(), None);
        assert_eq!(choose(Some("vote"), 1).unwrap(), Some(Shape::Vote));
        assert!(choose(Some("poll"), 2).is_err());
    }

    #[test]
    fn parts_read_back() {
        let t = "DECISION: Start the garden.\nCONFIDENCE: Medium (65%)\n**WHY**\n- x\nMINORITY VIEW: Bake bread first.\nHELD BY: Gemini";
        assert_eq!(parts(t), Parts { minority: Some("Bake bread first.".into()), held_by: Some("Gemini".into()), confidence: Some("medium".into()), decision: Some("Start the garden.".into()), ..Default::default() });
        let nested = parts("NEXT STEPS:\n1. Today:\n   - Write one line.\n   - List entities.\n2. Map workflows.\n3. Apply the test.\n**WHY**\n- x");
        assert_eq!(nested.next_steps, ["Today:; Write one line.; List entities.", "Map workflows.", "Apply the test."]);
        let t = "QUESTIONS FIRST:\n1. What is the workload?\n2) How many users?\n\nDECISION: If reads dominate, Postgres.\nCONFIDENCE: low (40%)\nWOULD CHANGE IT: Heavy writes.\n**NEXT STEPS:**\n1. Count the queries.\n2. Price both.\n- Pick one.\n**WHY**\n- x\nMINORITY VIEW: None\nHELD BY: Nobody";
        let p = parts(t);
        assert_eq!(p.questions, ["What is the workload?", "How many users?"]);
        assert_eq!(p.next_steps, ["Count the queries.", "Price both.; Pick one."]);
        assert_eq!(p.would_change.as_deref(), Some("Heavy writes."));
        assert_eq!(p.decision.as_deref(), Some("If reads dominate, Postgres."));
        assert_eq!(p.confidence.as_deref(), Some("low"));
        let none = parts("RESULT: tie\nMINORITY VIEW: None\nHELD BY: Nobody");
        assert_eq!(none, Parts::default());
    }
}
