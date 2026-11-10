//! The question coach and the suggested panel. Same prompts and rules as `lib/coach.py`.
//!
//! The coach is one AI, the chair's seat (Claude through the signed-in app, then the key).
//! It asks at most three short clarifying questions, then rewrites the question and the
//! answers into a brief: goal, context, constraints, and what a good answer looks like.
//! The table reads the brief in front of the question.
//!
//! The suggested panel picks persona seats for a question or brief. The chair picks when it
//! can; otherwise a word match on each persona's name, expertise and brief picks. Both
//! engines use the same word list, so they suggest the same seats offline.

use crate::personas::{Library, Persona};
use serde::{Deserialize, Serialize};

/// Most clarifying questions the coach asks.
pub const MAX_QUESTIONS: usize = 3;
/// Seats the panel suggests, at most.
pub const MAX_SUGGESTED: usize = 4;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Brief {
    pub goal: String,
    pub context: String,
    pub constraints: String,
    /// What a good answer looks like.
    pub good_answer: String,
}

const FIELDS: [(&str, &str); 4] = [("GOAL", "Goal"), ("CONTEXT", "Context"), ("CONSTRAINTS", "Constraints"), ("GOOD ANSWER", "A good answer")];

impl Brief {
    fn get(&self, i: usize) -> &str {
        match i {
            0 => &self.goal,
            1 => &self.context,
            2 => &self.constraints,
            _ => &self.good_answer,
        }
    }
    fn set(&mut self, i: usize, v: String) {
        match i {
            0 => self.goal = v,
            1 => self.context = v,
            2 => self.constraints = v,
            _ => self.good_answer = v,
        }
    }

    /// The brief as the table reads it: one list item per field that has text.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        for (i, (_, label)) in FIELDS.iter().enumerate() {
            let v = one_line(self.get(i));
            if !v.is_empty() {
                out.push_str(&format!("- **{label}:** {v}\n"));
            }
        }
        out.trim_end().to_string()
    }

    pub fn is_empty(&self) -> bool {
        (0..4).all(|i| self.get(i).trim().is_empty())
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The prompt that asks the coach for clarifying questions.
pub fn questions_prompt(question: &str) -> String {
    format!(
        "You are the CHAIR of an AI council. Before the council answers, help the person sharpen their question.\n\nTHEIR QUESTION:\n{}\n\n--- YOUR JOB ---\nAsk at most {MAX_QUESTIONS} short clarifying questions whose answers would change the advice most: the goal, the situation, the limits, or what a good answer looks like. Each question is one sentence of at most 20 words. Reply with only the questions, one per line, each starting with \"Q: \". If the question is already clear, reply with exactly \"NONE\".\n",
        question.trim()
    )
}

/// The coach's questions, from its reply. At most three; none when it said NONE.
pub fn parse_questions(reply: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in reply.lines() {
        let mut t = line.trim().trim_start_matches(['-', '*', '#']).trim();
        // "1." or "2)" numbering
        if let Some(rest) = t.split_once(['.', ')']).filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
            t = rest.1.trim();
        }
        let up = t.to_ascii_uppercase();
        let q = if up.starts_with("Q:") {
            t[2..].trim()
        } else if up.starts_with("QUESTION:") {
            t[9..].trim()
        } else if t.ends_with('?') && !up.starts_with("NONE") {
            t
        } else {
            continue;
        };
        let q = q.trim_matches('*').trim();
        if !q.is_empty() && !out.iter().any(|x: &String| x == q) {
            out.push(q.to_string());
        }
        if out.len() == MAX_QUESTIONS {
            break;
        }
    }
    out
}

/// The prompt that checks a question for missing key facts before the table answers.
/// Same text as `clarify_prompt` in `lib/coach.py`.
pub fn clarify_prompt(question: &str) -> String {
    format!(
        "You are the CHAIR of an AI council. Before the council answers, check the question for missing key facts.\n\nTHEIR QUESTION:\n{}\n\n--- YOUR JOB ---\nDoes the right pick flip on facts the person did not give, such as their workload, size, budget, deadline, or the rules they must follow? Ask only when it does: most questions have a sound pick under stated assumptions, and for those reply with exactly \"NONE\". When it does, ask the two or three questions whose answers would change the pick most, one per line, each starting with \"Q: \", each one sentence of at most 20 words.\n",
        question.trim()
    )
}

/// Fewest questions that count as "key facts are missing".
pub const MIN_CLARIFY: usize = 2;

/// The chair's open questions when the question lacks key facts: two or three, or none.
pub fn clarify_questions(reply: &str) -> Vec<String> {
    let qs = parse_questions(reply);
    if qs.len() >= MIN_CLARIFY {
        qs
    } else {
        Vec::new()
    }
}

/// The prompt that turns the question and the answers into a brief.
pub fn brief_prompt(question: &str, qa: &[(String, String)]) -> String {
    let mut p = format!("You are the CHAIR of an AI council. Rewrite the person's question into a brief the council will read.\n\nTHEIR QUESTION:\n{}\n", question.trim());
    let answered: Vec<_> = qa.iter().filter(|(_, a)| !a.trim().is_empty()).collect();
    if !answered.is_empty() {
        p.push_str("\nTHEIR ANSWERS TO YOUR CLARIFYING QUESTIONS:\n");
        for (q, a) in answered {
            p.push_str(&format!("Q: {}\nA: {}\n", one_line(q), one_line(a)));
        }
    }
    p.push_str("\n--- YOUR JOB ---\nReply with exactly four lines and nothing else:\nGOAL: <what they want to decide or achieve, in one sentence>\nCONTEXT: <the facts that matter, in one or two sentences>\nCONSTRAINTS: <limits on money, time, people or risk, or \"None given\">\nGOOD ANSWER: <what a useful answer looks like: its form and what it must cover>\nUse only what they said. Do not invent facts.\n");
    p
}

/// Read the four brief lines back. Missing fields stay empty; with no goal, the question is the goal.
pub fn parse_brief(reply: &str, question: &str) -> Brief {
    let mut b = Brief::default();
    let mut cur: Option<usize> = None;
    for line in reply.lines() {
        let t = line.trim().trim_start_matches(['-', '*']).trim().replace("**", "");
        let t = t.trim();
        let up = t.to_ascii_uppercase();
        let hit = FIELDS.iter().enumerate().find_map(|(i, (key, label))| {
            [key.to_string(), label.to_ascii_uppercase(), if i == 3 { "WHAT A GOOD ANSWER LOOKS LIKE".into() } else { String::new() }]
                .into_iter()
                .filter(|k| !k.is_empty())
                .find(|k| up.starts_with(&format!("{k}:")))
                .map(|k| (i, t[k.len() + 1..].trim().to_string()))
        });
        match hit {
            Some((i, v)) => {
                b.set(i, v);
                cur = Some(i);
            }
            None if !t.is_empty() => {
                if let Some(i) = cur {
                    let v = format!("{} {}", b.get(i), t);
                    b.set(i, v.trim().to_string());
                }
            }
            None => {}
        }
    }
    if b.goal.trim().is_empty() {
        b.goal = one_line(question);
    }
    b
}

/// A brief someone edited by hand, from its Markdown (the `to_markdown` shape) or as plain text.
/// Plain text that is not in the four-field shape stays as the goal.
pub fn brief_from_text(text: &str) -> Brief {
    let b = parse_brief(text, "");
    if b.context.is_empty() && b.constraints.is_empty() && b.good_answer.is_empty() && !text.to_ascii_uppercase().contains("GOAL:") {
        return Brief { goal: one_line(text), ..Brief::default() };
    }
    b
}

// ---- The suggested panel ----

const STOP: &[&str] = &[
    "about", "after", "again", "also", "because", "before", "being", "best", "could", "does", "doing", "from", "good", "have", "help", "into", "just",
    "like", "make", "more", "most", "need", "only", "other", "over", "really", "right", "should", "some", "than", "that", "their", "them", "then",
    "there", "these", "they", "think", "this", "those", "very", "want", "were", "what", "when", "where", "which", "while", "will", "with", "would",
    "your", "ours", "yours", "answer", "goal", "context", "constraints", "none", "given", "looks",
];

/// Words that carry meaning: lower case, four letters or more, not in the stop list.
pub fn words(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in s.to_ascii_lowercase().split(|c: char| !c.is_ascii_alphanumeric()) {
        if w.len() >= 4 && !STOP.contains(&w) && !out.iter().any(|x| x == w) {
            out.push(w.to_string());
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggestion {
    /// Persona ids, in seat order. Empty means a Generic table.
    pub personas: Vec<String>,
    /// One plain line on why.
    pub why: String,
    /// "chair", "words" or "none".
    pub source: String,
}

/// Pick seats by word match. Deterministic, so both engines agree offline.
pub fn suggest_by_words(text: &str, lib: &Library) -> Suggestion {
    let q = words(text);
    let mut scored: Vec<(usize, &Persona, Vec<String>)> = Vec::new();
    for p in lib.packs.iter().flat_map(|p| &p.personas) {
        if scored.iter().any(|(_, x, _)| x.id == p.id) {
            continue;
        }
        let pw = words(&format!("{} {} {}", p.name, p.expertise, p.brief));
        let hits: Vec<String> = q.iter().filter(|w| pw.contains(w)).cloned().collect();
        if !hits.is_empty() {
            scored.push((hits.len(), p, hits));
        }
    }
    // Stable: library order breaks ties.
    scored.sort_by_key(|x| std::cmp::Reverse(x.0));
    scored.truncate(MAX_SUGGESTED);
    if scored.len() < 2 {
        return Suggestion { personas: Vec::new(), why: "No persona matched the question closely, so each AI answers as itself.".into(), source: "none".into() };
    }
    let mut matched: Vec<String> = Vec::new();
    for (_, _, h) in &scored {
        for w in h {
            if !matched.contains(w) {
                matched.push(w.clone());
            }
        }
    }
    matched.truncate(6);
    Suggestion {
        personas: scored.iter().map(|(_, p, _)| p.id.clone()).collect(),
        why: format!("Matched on: {}.", matched.join(", ")),
        source: "words".into(),
    }
}

/// The prompt that asks the chair to pick seats.
pub fn suggest_prompt(text: &str, lib: &Library) -> String {
    let mut p = format!("You are the CHAIR of an AI council. Pick the seats for this question.\n\nTHE QUESTION:\n{}\n\nPERSONAS (id: name, expertise):\n", text.trim());
    let mut seen: Vec<&str> = Vec::new();
    for x in lib.packs.iter().flat_map(|p| &p.personas) {
        if seen.contains(&x.id.as_str()) {
            continue;
        }
        seen.push(&x.id);
        p.push_str(&format!("- {}: {}, {}\n", x.id, x.name, one_line(&x.expertise)));
    }
    p.push_str(&format!("\n--- YOUR JOB ---\nPick 2 to {MAX_SUGGESTED} persona ids whose views matter most for this question, and who will disagree usefully. Reply with exactly two lines:\nSEATS: <ids, comma separated>\nWHY: <one sentence>\n"));
    p
}

/// The chair's pick, checked against the library. None when it named no known persona.
pub fn parse_suggestion(reply: &str, lib: &Library) -> Option<Suggestion> {
    let mut ids: Vec<String> = Vec::new();
    let mut why = String::new();
    for line in reply.lines() {
        let t = line.trim().replace("**", "");
        let up = t.to_ascii_uppercase();
        if let Some(rest) = up.starts_with("SEATS:").then(|| t[6..].to_string()) {
            for id in rest.split(',').map(|s| s.trim().trim_matches(['`', '.', '"'])).filter(|s| !s.is_empty()) {
                if let Some(p) = lib.persona(id) {
                    if !ids.contains(&p.id) {
                        ids.push(p.id.clone());
                    }
                }
            }
        } else if up.starts_with("WHY:") {
            why = t[4..].trim().to_string();
        }
    }
    ids.truncate(MAX_SUGGESTED);
    if ids.is_empty() {
        return None;
    }
    Some(Suggestion { personas: ids, why, source: "chair".into() })
}

// ---- With the chair ----

use crate::keys::Keys;
use crate::settings::Settings;
use std::time::Duration;

/// How long the coach and the panel wait for the chair.
pub const CHAIR_WAIT: Duration = Duration::from_secs(180);

/// The coach's questions. Err when the chair's seat did not answer at all.
pub fn ask_questions(question: &str, keys: &Keys, settings: &Settings, timeout: Duration) -> Result<Vec<String>, String> {
    let reply = crate::council::chair(keys, settings, &questions_prompt(question), timeout);
    if reply.trim().is_empty() {
        return Err("The coach did not answer. Connect Claude, or skip the coach.".into());
    }
    Ok(parse_questions(&reply))
}

/// Clarify first: the chair's open questions when the question lacks key facts.
/// Empty when it has them, or when the chair did not answer.
pub fn clarify(question: &str, keys: &Keys, settings: &Settings, timeout: Duration) -> Vec<String> {
    clarify_questions(&crate::council::chair(keys, settings, &clarify_prompt(question), timeout))
}

/// The brief. Err when the chair's seat did not answer.
pub fn make_brief(question: &str, qa: &[(String, String)], keys: &Keys, settings: &Settings, timeout: Duration) -> Result<Brief, String> {
    let reply = crate::council::chair(keys, settings, &brief_prompt(question, qa), timeout);
    if reply.trim().is_empty() {
        return Err("The coach did not answer. Connect Claude, or skip the coach.".into());
    }
    Ok(parse_brief(&reply, question))
}

/// Seats for a question or brief: the chair's pick, else the word match.
/// `ROUNDTABLE_SUGGEST=words` skips the chair.
pub fn suggest(text: &str, lib: &Library, keys: &Keys, settings: &Settings, timeout: Duration) -> Suggestion {
    let words_only = std::env::var("ROUNDTABLE_SUGGEST").is_ok_and(|v| v.trim().eq_ignore_ascii_case("words"));
    if !words_only {
        let reply = crate::council::chair(keys, settings, &suggest_prompt(text, lib), timeout);
        if let Some(s) = parse_suggestion(&reply, lib) {
            return s;
        }
    }
    suggest_by_words(text, lib)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personas::{builtin_packs, Library};

    #[test]
    fn questions_are_capped_and_cleaned() {
        let r = "Q: What is the budget?\n2. Q: Who decides?\n- Q: By when?\nQ: Anything else?\n";
        assert_eq!(parse_questions(r), vec!["What is the budget?", "Who decides?", "By when?"]);
        assert!(parse_questions("NONE").is_empty());
        assert_eq!(parse_questions("Sure.\n1) How big is the team?"), vec!["How big is the team?"]);
    }

    #[test]
    fn clarify_needs_two_questions() {
        assert_eq!(clarify_questions("Q: What is the workload?\nQ: How many users?"), vec!["What is the workload?", "How many users?"]);
        assert!(clarify_questions("Q: Only one?").is_empty(), "one question is not enough to hold the pick");
        assert!(clarify_questions("NONE").is_empty());
        assert!(clarify_questions("").is_empty());
        let p = clarify_prompt(" Postgres or Mongo? ");
        assert!(p.contains("THEIR QUESTION:\nPostgres or Mongo?\n") && p.contains("\"NONE\"") && !p.contains('\u{2014}'));
    }

    #[test]
    fn brief_round_trips() {
        let reply = "GOAL: Pick a database.\nCONTEXT: A small web service.\nCONSTRAINTS: None given\nGOOD ANSWER: One pick, with the main risk.";
        let b = parse_brief(reply, "Postgres or SQLite?");
        assert_eq!(b.goal, "Pick a database.");
        assert_eq!(b.good_answer, "One pick, with the main risk.");
        let md = b.to_markdown();
        assert_eq!(md, "- **Goal:** Pick a database.\n- **Context:** A small web service.\n- **Constraints:** None given\n- **A good answer:** One pick, with the main risk.");
        assert_eq!(brief_from_text(&md), b);
        assert_eq!(parse_brief("", "Bread or garden?").goal, "Bread or garden?");
        assert_eq!(brief_from_text("Just this").goal, "Just this");
    }

    #[test]
    fn prompts_have_no_em_dashes() {
        for p in [questions_prompt("Q"), brief_prompt("Q", &[("a?".into(), "b".into())])] {
            assert!(!p.contains('\u{2014}'));
        }
    }

    #[test]
    fn word_match_suggests_finance_seats() {
        let lib = Library::from_packs(builtin_packs(), true);
        let s = suggest_by_words("Should we raise prices for new customers? Budget and revenue matter.", &lib);
        assert_eq!(s.source, "words");
        assert!(s.personas.len() >= 2 && s.personas.len() <= MAX_SUGGESTED, "{s:?}");
        assert!(s.personas.iter().any(|p| p == "cfo"), "{s:?}");
        let none = suggest_by_words("Bread or garden?", &lib);
        assert!(none.personas.is_empty());
    }

    #[test]
    fn chair_pick_is_checked() {
        let lib = Library::from_packs(builtin_packs(), true);
        let s = parse_suggestion("SEATS: cfo, nobody, ceo\nWHY: Money and direction.", &lib).unwrap();
        assert_eq!(s.personas, vec!["cfo", "ceo"]);
        assert_eq!(s.why, "Money and direction.");
        assert!(parse_suggestion("I would pick finance people.", &lib).is_none());
    }
}
