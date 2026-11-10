//! The council: same flow and transcript as `lib/core.sh`.
//!
//! Advisory (`rounds <= 1`): each head answers once, blind.
//! Deliberation (`rounds >= 2`): round 1 blind; later rounds see the whole table;
//! a Claude chair says CONSENSUS or CONTINUE after each round and the run stops early on consensus.

use crate::clis::{self, Fix};
use crate::http::{self, ErrorClass};
use crate::keys::Keys;
use crate::mode::{self, Mode};
use crate::personas::{self, Seat};
use crate::providers::{self, Head, Kind, HEADS};
use crate::settings::Settings;
use crate::verdict::{self, Shape};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct AskOptions {
    pub question: String,
    /// Background text every head reads first.
    pub context: Option<String>,
    /// Earlier turns of this session (a follow-up). Read before everything else.
    pub prior: Option<String>,
    /// Files and links, read once and shared (`attach::block`).
    pub attachments: Option<String>,
    /// The coach's brief: goal, context, constraints, what a good answer looks like.
    pub brief: Option<String>,
    /// Round 1 is blind by default. True lets each seat in round 1 read the seats before it.
    pub open_first_round: bool,
    /// The chair's last word, in this shape. None: no verdict (the old transcript).
    pub verdict: Option<Shape>,
    /// Clarify first: with no brief, the chair checks the question for missing key facts
    /// and, when the pick flips on them, the table and the verdict open with its questions.
    pub clarify: bool,
    pub heads: Vec<&'static Head>,
    /// Who sits where. Empty means one Generic seat per head (the old table).
    pub seats: Vec<Seat>,
    pub rounds: u32,
    pub research: bool,
    pub timeout: Duration,
    /// Quick (the default) or Deep Think: seat length, the verdict's length, and when the clarify check runs.
    pub mode: Mode,
}

/// Most rounds a run may have.
pub const MAX_ROUNDS: u32 = 4;

impl AskOptions {
    /// The seats this run uses.
    pub fn table(&self) -> Vec<Seat> {
        if self.seats.is_empty() {
            personas::generic_seats(&self.heads)
        } else {
            self.seats.clone()
        }
    }
}

/// One seat as the app sees it before any answer.
#[derive(Debug, Clone, Serialize)]
pub struct SeatInfo {
    pub seat: String,
    pub head: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expertise: Option<String>,
    /// The lab label, on lab seats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Answered(String),
    /// No key and no local tool: what to do, in one line.
    Missing(String, Fix),
    Timeout,
    Empty(String),
    Failed(ErrorClass, String),
    /// The person's own tool ran and did not answer, with the reason in plain words.
    ToolFailed(Fix, String),
}

impl Outcome {
    /// The text the transcript shows for this head (same sentinels as `body_of_file`).
    pub fn body(&self) -> String {
        match self {
            Outcome::Answered(t) => t.clone(),
            Outcome::Missing(..) => "_(unavailable, skipped)_".into(),
            Outcome::Timeout => "_(timed out)_".into(),
            Outcome::Empty(_) => "_(no answer)_".into(),
            Outcome::Failed(c, _) => format!("_(no answer: {})_", c.message()),
            Outcome::ToolFailed(_, m) => format!("_(no answer: {m})_"),
        }
    }
    pub fn status(&self) -> &'static str {
        match self {
            Outcome::Answered(_) => "answered",
            Outcome::Missing(..) => "skipped",
            Outcome::Timeout => "timed_out",
            Outcome::Empty(_) => "failed",
            Outcome::Failed(..) => "failed",
            Outcome::ToolFailed(..) => "failed",
        }
    }
    /// One plain line for the card. None when it answered.
    pub fn message(&self) -> Option<String> {
        match self {
            Outcome::Answered(_) => None,
            Outcome::Missing(m, _) | Outcome::ToolFailed(_, m) => Some(m.clone()),
            Outcome::Timeout => Some(ErrorClass::Timeout.message().into()),
            Outcome::Empty(_) => Some("That AI sent an empty answer.".into()),
            Outcome::Failed(c, _) => Some(c.message().into()),
        }
    }
    /// The one thing to do about it.
    pub fn fix(&self) -> Option<Fix> {
        match self {
            Outcome::Answered(_) => None,
            Outcome::Missing(_, f) | Outcome::ToolFailed(f, _) => Some(*f),
            Outcome::Failed(ErrorClass::BadKey, _) => Some(Fix::AddKey),
            _ => Some(Fix::Retry),
        }
    }
    pub fn reason(&self) -> Option<ErrorClass> {
        match self {
            Outcome::Failed(c, _) => Some(*c),
            Outcome::Timeout => Some(ErrorClass::Timeout),
            _ => None,
        }
    }
}

/// One JSON line per event with `--events`. Drives the app's live cards.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    RunStarted {
        heads: Vec<String>,
        rounds: u32,
        mode: String,
        seats: Vec<SeatInfo>,
        /// Quick or Deep Think ("quick" or "deep").
        depth: Mode,
    },
    HeadStarted { head: String, round: u32, seat: String },
    HeadDone {
        head: String,
        round: u32,
        /// The seat key: the head id on a Generic seat, else `<persona>@<head>`.
        seat: String,
        status: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<ErrorClass>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// sign_in, add_key, install or retry.
        #[serde(skip_serializing_if = "Option::is_none")]
        fix: Option<Fix>,
        /// How it was asked: the tool's name, or "API key".
        #[serde(skip_serializing_if = "Option::is_none")]
        via: Option<String>,
    },
    RoundDone { round: u32 },
    /// The chair's open questions: the question lacks key facts the pick depends on.
    Clarify { questions: Vec<String> },
    ChairVerdict { round: u32, consensus: bool, text: String },
    /// The final verdict in the chosen shape, with the minority view and who held it.
    Verdict {
        shape: Shape,
        text: String,
        #[serde(flatten)]
        parts: verdict::Parts,
    },
    /// The decision model's rating of the verdict (only when one is on).
    Decision(crate::decision::Outcome),
    RunDone {
        transcript: String,
        answered: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        consensus: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        history_id: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub transcript: String,
    pub answered: usize,
    pub consensus: Option<bool>,
    /// The decision model's rating, when one is on.
    pub decision: Option<crate::decision::Outcome>,
}

/// Default per-head time limit: 300 s, or 360 s with web search.
pub fn default_timeout(research: bool, settings: &Settings) -> Duration {
    Duration::from_secs(settings.timeout_secs.unwrap_or(if research { 360 } else { 300 }))
}

/// Parse a comma list of heads. Unknown names come back as warnings.
pub fn parse_heads(csv: &str) -> (Vec<&'static Head>, Vec<String>) {
    let mut out = Vec::new();
    let mut warn = Vec::new();
    for name in csv.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let found = HEADS.iter().find(|h| h.id == name).or_else(|| crate::custom::head(name));
        match found {
            Some(h) if !out.iter().any(|o: &&Head| o.id == h.id) => out.push(h),
            Some(_) => {}
            None => warn.push(format!("WARN: unknown head '{name}'")),
        }
    }
    (out, warn)
}

/// Heads for a run with no `--heads`: settings, then ROUNDTABLE_HEADS, then every
/// head that can answer (has a key, or the local tool). Falls back to the full list.
pub fn default_heads(settings: &Settings, keys: &Keys) -> Vec<&'static Head> {
    if !settings.default_heads.is_empty() {
        let (h, _) = parse_heads(&settings.default_heads.join(","));
        if !h.is_empty() {
            return h;
        }
    }
    let env_heads = keys.env.get("ROUNDTABLE_HEADS").or_else(|| keys.file.get("ROUNDTABLE_HEADS"));
    if let Some(csv) = env_heads.filter(|v| !v.trim().is_empty()) {
        let (h, _) = parse_heads(csv);
        if !h.is_empty() {
            return h;
        }
    }
    let (all, _) = parse_heads(providers::DEFAULT_HEADS);
    let chosen = |h: &Head| settings.cli_heads.iter().any(|c| c == h.id);
    let mut ready: Vec<&'static Head> = HEADS
        .iter()
        .filter(|h| match h.kind {
            // codex sits by default when installed, as in the bash engine; other tools when chosen.
            Kind::Cli => (h.id == "codex" || chosen(h)) && cli_found(h),
            _ => keys.key(h).is_some() || ((chosen(h) || h.id == "grok") && cli_found(h)),
        })
        .collect();
    // Every ticked model of an added provider that can be asked now is a seat too.
    ready.extend(crate::custom::ticked_ready(&settings.providers, keys));
    if ready.is_empty() {
        all
    } else {
        ready
    }
}

pub fn base_prompt(question: &str, context: Option<&str>) -> String {
    prompt_with(question, None, context, None, None)
}

/// The shared prompt: earlier turns, background, attachments and the brief, then the question.
/// With none of them it is the question alone, as before.
pub fn prompt_with(question: &str, prior: Option<&str>, context: Option<&str>, attachments: Option<&str>, brief: Option<&str>) -> String {
    prompt_with_open(question, prior, context, attachments, brief, None)
}

/// Same, with the chair's open questions after the brief.
pub fn prompt_with_open(question: &str, prior: Option<&str>, context: Option<&str>, attachments: Option<&str>, brief: Option<&str>, open: Option<&str>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (title, v) in [("EARLIER IN THIS SESSION", prior), ("CONTEXT", context), ("ATTACHMENTS", attachments), ("BRIEF", brief), ("OPEN QUESTIONS", open)] {
        if let Some(v) = v.filter(|v| !v.trim().is_empty()) {
            parts.push(format!("## {title}\n{}", v.trim_end_matches('\n')));
        }
    }
    if parts.is_empty() {
        return question.to_string();
    }
    format!("{}\n\n## QUESTION\n{}", parts.join("\n\n"), question)
}

/// The prompt for one seat in an open first round: the answers of the seats before it.
pub fn open_round_prompt(base: &str, before: &[(String, String)]) -> String {
    if before.is_empty() {
        return base.to_string();
    }
    let mut p = format!("{base}\n\n---\n## ANSWERS SO FAR (round 1 is open): the seats that answered before you:\n\n");
    for (label, body) in before {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    p.push_str("## YOUR TASK\nGive your own answer. You may build on these answers or disagree with them; say which.\n");
    p
}

/// The chair's open questions as a prompt section the seats read before the question.
pub fn open_questions_section(questions: &[String]) -> Option<String> {
    if questions.is_empty() {
        return None;
    }
    let mut t = String::from("The chair would ask the person these first. They are not answered yet, so say how each answer would change your pick.\n");
    for q in questions {
        t.push_str(&format!("- {q}\n"));
    }
    Some(t.trim_end().to_string())
}

/// Round 2 and later: everyone's last position, the chair's reading of it, then the task:
/// argue against the leading pick first, settle every difference, then revise.
pub fn deliberation_prompt(base: &str, prev: u32, table: &[(String, String)], round: u32, chair: Option<&str>) -> String {
    let mut p = format!("{base}\n\n---\n## COUNCIL DELIBERATION: every councilor's position from round {prev}:\n\n");
    for (label, body) in table {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    if let Some(c) = chair.filter(|c| !c.trim().is_empty()) {
        p.push_str(&format!("## THE CHAIR'S READING OF ROUND {prev}\n{}\n\n", c.trim()));
    }
    p.push_str(&format!(
        "## YOUR TASK (round {round})\nYou are ONE councilor. You have now read everyone. First argue against the leading pick: under \"CASE AGAINST:\", give the strongest case that it is wrong for this person, in at most three bullets, even if you back it. Then settle every difference in numbers, thresholds or first steps: give your figure or step and why. Then update your position. State explicitly: (a) where a peer changed your mind; name them; (b) where you still disagree and why; (c) your sharpened current answer, under \"POSITION:\". Begin your reply with \"CASE AGAINST:\".\n"
    ));
    p
}

pub fn chair_prompt(question: &str, round: u32, table: &[(String, String)]) -> String {
    let mut p = format!(
        "You are the neutral CHAIR of an AI design council. The councilors submitted these round {round} positions to this question:\n\n{question}\n\n--- POSITIONS ---\n\n"
    );
    for (label, body) in table {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    p.push_str("--- YOUR JOB ---\nLine 1 must be EXACTLY one of: \"VERDICT: CONSENSUS\" or \"VERDICT: CONTINUE\". Line 2: \"LEADING PICK: <the option most seats back, in one sentence>\". Then give: **AGREED** (bullets) and **REMAINING DISAGREEMENTS** (bullets; name who holds what). Then **DIFFERENCES**: one bullet for each place the seats give a different number (a price, cost, size, date or percentage), a different threshold or decision rule, or a different first step, naming who said what; write \"- none\" when there are none. CONSENSUS only on true agreement: the same pick, the same numbers and thresholds (within rounding), and the same first step. Any bullet under DIFFERENCES means CONTINUE.\n");
    p
}

/// True when line 1 reads `VERDICT: CONSENSUS` (any case, any spaces after the colon).
pub fn is_consensus(chair: &str) -> bool {
    let first = chair.lines().next().unwrap_or("").to_ascii_lowercase();
    match first.find("verdict:") {
        Some(i) => first[i + "verdict:".len()..].trim_start().starts_with("consensus"),
        None => false,
    }
}

/// The bullets under the chair's DIFFERENCES heading, without "none".
pub fn differences(chair: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in chair.lines() {
        let t = line.trim().replace("**", "");
        let t = t.trim();
        let up = t.to_ascii_uppercase();
        if up.starts_with("DIFFERENCES") {
            inside = true;
            let rest = t["DIFFERENCES".len()..].trim_start_matches(':').trim();
            if !rest.is_empty() && !is_none(rest) {
                out.push(rest.to_string());
            }
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            if !is_none(b) {
                out.push(b.trim().to_string());
            }
        } else if !t.is_empty() {
            inside = false;
        }
    }
    out
}

fn is_none(s: &str) -> bool {
    let s = s.trim().trim_matches(['.', '"', '(', ')', '_', '*']).to_ascii_lowercase();
    s == "none" || s == "no differences" || s == "nothing"
}

/// True agreement: the chair says CONSENSUS and lists no difference in numbers,
/// thresholds or first steps. Anything less runs another round.
pub fn agreed(chair: &str) -> bool {
    is_consensus(chair) && differences(chair).is_empty()
}

/// Look for an executable on PATH and in the usual per-user folders.
pub fn find_exe(name: &str) -> Option<PathBuf> {
    clis::find_exe(name)
}

fn openai_compat(head: &Head, key: &str, prompt: &str, research: bool, settings: &Settings, timeout: Duration) -> Outcome {
    let glm = if head.id == "glm" { Some(research) } else { None };
    let body = http::chat_body(&providers::model_for(head, settings), prompt, head.max_tokens, glm);
    match http::send("POST", &providers::url_for(head), &http::bearer(key), Some(&body), timeout) {
        Ok(v) => {
            let mut t = http::chat_text(&v);
            if head.id == "minimax" {
                t = http::strip_think(&t);
            }
            if t.is_empty() {
                Outcome::Empty(format!("{}: empty answer", head.id))
            } else {
                Outcome::Answered(t)
            }
        }
        Err(e) if e.class == ErrorClass::Timeout => Outcome::Timeout,
        Err(e) => Outcome::Failed(e.class, e.detail),
    }
}

/// Anthropic call for the Claude head and the chair.
pub fn anthropic(key: &str, model: &str, prompt: &str, timeout: Duration) -> Outcome {
    let url = providers::url_for(providers::find("claude").unwrap());
    let body = http::anthropic_body(model, prompt, 8000);
    match http::send("POST", &url, &http::anthropic_headers(key), Some(&body), timeout) {
        Ok(v) => {
            let t = http::anthropic_text(&v);
            if t.is_empty() {
                Outcome::Empty("claude: empty answer".into())
            } else {
                Outcome::Answered(t)
            }
        }
        Err(e) if e.class == ErrorClass::Timeout => Outcome::Timeout,
        Err(e) => Outcome::Failed(e.class, e.detail),
    }
}

fn cli_found(head: &Head) -> bool {
    clis::for_head(head.id).is_some_and(|c| clis::find_cli(c).is_some())
}

/// Which way to ask a head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// The person's own signed-in tool first, then the key.
    Auto,
    /// Only the tool (the Test button on an app card).
    Tool,
    /// Only the key (the Test button on a key card).
    Key,
}

/// One head's answer, and how it was asked.
#[derive(Debug, Clone)]
pub struct HeadRun {
    pub outcome: Outcome,
    /// The tool's name, or "API key". None when nothing could ask.
    pub via: Option<String>,
    /// What the tool ran, when a tool ran.
    pub trial: Option<clis::Trial>,
}

pub const VIA_KEY: &str = "API key";

/// What to tell a person when a head has neither a tool nor a key.
pub fn missing(head: &Head, route: Route) -> Outcome {
    let tool = clis::for_head(head.id);
    let keyed = head.kind != Kind::Cli;
    match (route, tool) {
        (Route::Tool, Some(c)) => Outcome::Missing(format!("{} is not on this computer.", c.name), Fix::Install),
        (Route::Tool, None) => Outcome::Missing(format!("{} has no app to use. Add a key instead.", head.display), Fix::AddKey),
        (Route::Key, _) | (Route::Auto, None) if keyed => Outcome::Missing(format!("Add a {} key.", head.display), Fix::AddKey),
        // The tool is not installed (a signed-out tool runs and says so itself).
        (Route::Auto, Some(c)) if keyed => Outcome::Missing(format!("Add a {} key or install {}.", head.display, c.name), Fix::AddKey),
        (_, Some(c)) => Outcome::Missing(format!("Install {} to seat {}.", c.name, head.display), Fix::Install),
        (_, None) => Outcome::Missing(format!("{} cannot be asked on this computer.", head.display), Fix::Install),
    }
}

fn via_key(head: &Head, key: &str, prompt: &str, research: bool, settings: &Settings, timeout: Duration, model: Option<&str>) -> Outcome {
    match head.kind {
        Kind::Anthropic => anthropic(key, &model.map(str::to_string).unwrap_or_else(|| providers::model_for(head, settings)), prompt, timeout),
        Kind::OpenAiCompat => openai_compat(head, key, prompt, research, settings, timeout),
        Kind::Cli => missing(head, Route::Key),
        // `ask_head` asks provider seats before it gets here.
        Kind::Provider => missing(head, Route::Key),
    }
}

/// Ask one head by the route that works: the person's own signed-in tool first,
/// the API key second. Skipped only when it has neither.
/// `key_model` overrides the key's model (the chair asks Claude with its own model).
#[allow(clippy::too_many_arguments)]
pub fn ask_head(head: &Head, prompt: &str, research: bool, timeout: Duration, keys: &Keys, settings: &Settings, route: Route, key_model: Option<&str>) -> HeadRun {
    // A provider seat: one plain chat completion, no tools (the rc8 seat isolation rule).
    if head.kind == Kind::Provider {
        let (outcome, via) = crate::custom::ask(head, prompt, timeout, keys);
        return HeadRun { outcome, via: Some(via).filter(|v| !v.is_empty()), trial: None };
    }
    let tool = if route == Route::Key { None } else { clis::for_head(head.id).and_then(|c| clis::find_cli(c).map(|e| (c, e))) };
    let key = if route == Route::Tool || head.kind == Kind::Cli { None } else { keys.key(head) };
    let mut tried: Option<HeadRun> = None;
    if let Some((c, exe)) = tool {
        let (r, trial) = clis::ask_detailed(c, &exe, prompt, research, timeout);
        let outcome = match r {
            clis::CliResult::Answered(t) => Outcome::Answered(t),
            clis::CliResult::Missing => missing(head, Route::Tool),
            clis::CliResult::Timeout => Outcome::Timeout,
            clis::CliResult::Failed(f) => Outcome::ToolFailed(f.fix, f.message),
        };
        let run = HeadRun { outcome, via: Some(c.name.to_string()), trial: Some(trial) };
        if matches!(run.outcome, Outcome::Answered(_)) || key.is_none() {
            return run;
        }
        tried = Some(run);
    }
    if let Some(k) = key {
        let outcome = via_key(head, &k, prompt, research, settings, timeout, key_model);
        // Both failed: the tool's reason is usually the one to act on, unless the key was refused.
        if let (Some(t), false) = (&tried, matches!(outcome, Outcome::Answered(_))) {
            if !matches!(outcome, Outcome::Failed(ErrorClass::BadKey, _)) {
                return t.clone();
            }
        }
        return HeadRun { outcome, via: Some(VIA_KEY.into()), trial: tried.and_then(|t| t.trial) };
    }
    HeadRun { outcome: missing(head, route), via: None, trial: None }
}

/// Ask one head with the run's options.
pub fn run_head(head: &Head, prompt: &str, opts: &AskOptions, keys: &Keys, settings: &Settings) -> Outcome {
    ask_head(head, prompt, opts.research, opts.timeout, keys, settings, Route::Auto, None).outcome
}

/// The chair: Claude by the same route as a head, with the chair's model on a key.
/// With no way to ask Claude at all, the first ticked provider seat that can be asked
/// chairs instead, so the coach, the panel and the verdict work on a provider-only table.
pub fn chair(keys: &Keys, settings: &Settings, prompt: &str, timeout: Duration) -> String {
    let claude = providers::find("claude").unwrap();
    match ask_head(claude, prompt, false, timeout, keys, settings, Route::Auto, Some(&providers::chair_model(settings))).outcome {
        Outcome::Answered(t) => t,
        Outcome::Missing(..) => match crate::custom::ticked_ready(&settings.providers, keys).first() {
            Some(h) => match crate::custom::ask(h, prompt, timeout, keys).0 {
                Outcome::Answered(t) => t,
                _ => String::new(),
            },
            None => String::new(),
        },
        _ => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_round(prompt: &str, round: u32, seats: &[Seat], keys_of: &[String], opts: &AskOptions, keys: &Keys, settings: &Settings, emit: &(dyn Fn(&Event) + Sync)) -> Vec<HeadRun> {
    std::thread::scope(|s| {
        let handles: Vec<_> = seats
            .iter()
            .zip(keys_of)
            .map(|(seat, key)| {
                let h = seat.head;
                emit(&Event::HeadStarted { head: h.id.into(), round, seat: key.clone() });
                s.spawn(move || {
                    let p = personas::seat_prompt(seat, &mode::with_rule(prompt, opts.mode));
                    let r = ask_head(h, &p, opts.research, opts.timeout, keys, settings, Route::Auto, None);
                    emit(&head_done_at(h, key, round, &r));
                    r
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or(HeadRun { outcome: Outcome::Empty("panic".into()), via: None, trial: None }))
            .collect()
    })
}

/// An open first round: seats answer one after another, each reading the ones before it.
#[allow(clippy::too_many_arguments)]
fn run_open_round(base: &str, seats: &[Seat], keys_of: &[String], opts: &AskOptions, keys: &Keys, settings: &Settings, emit: &(dyn Fn(&Event) + Sync)) -> Vec<HeadRun> {
    for (seat, key) in seats.iter().zip(keys_of) {
        emit(&Event::HeadStarted { head: seat.head.id.into(), round: 1, seat: key.clone() });
    }
    let mut out: Vec<HeadRun> = Vec::new();
    let mut before: Vec<(String, String)> = Vec::new();
    for (seat, key) in seats.iter().zip(keys_of) {
        let p = personas::seat_prompt(seat, &mode::with_rule(&open_round_prompt(base, &before), opts.mode));
        let r = ask_head(seat.head, &p, opts.research, opts.timeout, keys, settings, Route::Auto, None);
        emit(&head_done_at(seat.head, key, 1, &r));
        if let Outcome::Answered(t) = &r.outcome {
            before.push((personas::seat_label(seat, &label_for(seat.head, settings, &r)), t.clone()));
        }
        out.push(r);
    }
    out
}

/// The `head_done` event for one head's result.
pub fn head_done(h: &Head, round: u32, r: &HeadRun) -> Event {
    head_done_at(h, h.id, round, r)
}

/// The `head_done` event for one seat's result.
pub fn head_done_at(h: &Head, seat: &str, round: u32, r: &HeadRun) -> Event {
    let o = &r.outcome;
    Event::HeadDone {
        head: h.id.into(),
        round,
        seat: seat.into(),
        status: o.status().into(),
        text: if let Outcome::Answered(t) = o { t.clone() } else { String::new() },
        reason: o.reason(),
        message: o.message(),
        fix: o.fix(),
        via: r.via.clone(),
    }
}

/// Transcript label: says "your plan" when the head answered through the person's tool.
fn label_for(h: &Head, settings: &Settings, r: &HeadRun) -> String {
    let tool = r.via.as_deref().is_some_and(|v| v != VIA_KEY);
    providers::label_via(h, settings, tool)
}

/// A seat's body in the transcript: the lab label first on a lab seat.
fn seat_body(seat: &Seat, body: &str) -> String {
    match personas::seat_disclaimer(seat) {
        Some(d) => format!("_{d}_\n\n{body}"),
        None => body.to_string(),
    }
}

/// The verdict, when the run asks for one: the heading and the chair's text.
fn verdict_section(opts: &AskOptions, keys: &Keys, settings: &Settings, open: &[String], finals: &[(String, String)], emit: &(dyn Fn(&Event) + Sync)) -> Option<(String, String)> {
    let shape = opts.verdict?;
    let text = chair(keys, settings, &verdict::prompt(shape, opts.mode, &opts.question, opts.brief.as_deref(), opts.attachments.as_deref(), open, finals), opts.timeout);
    let text = if text.trim().is_empty() { verdict::UNAVAILABLE.to_string() } else { text.trim().to_string() };
    emit(&Event::Verdict { shape, parts: verdict::parts(&text), text: text.clone() });
    Some((format!("## \u{1fa91} Verdict ({})\n\n{text}\n", shape.title().to_ascii_lowercase()), text))
}

/// The summary line of the folded seat answers.
pub fn fold_summary(n: usize, rounds_run: u32) -> String {
    let seats = if n == 1 { "1 seat".to_string() } else { format!("{n} seats") };
    if rounds_run <= 1 {
        format!("<details><summary>The seat answers ({seats})</summary>")
    } else {
        format!("<details><summary>The seat answers and the rounds ({seats}, {rounds_run} rounds)</summary>")
    }
}

/// Run the council. `emit` gets every event as it happens; the transcript comes back.
/// With a verdict, the transcript leads with it and the seat answers sit folded under it.
pub fn run(opts: &AskOptions, keys: &Keys, settings: &Settings, emit: &(dyn Fn(&Event) + Sync)) -> RunResult {
    let seats = opts.table();
    let seat_keys = personas::seat_keys(&seats);
    let n = seats.len();
    let rounds = opts.rounds.clamp(1, MAX_ROUNDS);
    let advisory = rounds <= 1;
    let mut heads_seen: Vec<String> = Vec::new();
    for s in &seats {
        if !heads_seen.iter().any(|h| h == s.head.id) {
            heads_seen.push(s.head.id.to_string());
        }
    }
    emit(&Event::RunStarted {
        heads: heads_seen,
        rounds,
        mode: if advisory { "advisory".into() } else { "deliberation".into() },
        seats: seats
            .iter()
            .zip(&seat_keys)
            .map(|(s, k)| SeatInfo {
                seat: k.clone(),
                head: s.head.id.into(),
                persona: s.persona.as_ref().map(|p| p.name.clone()),
                expertise: s.persona.as_ref().map(|p| p.expertise.clone()).filter(|e| !e.is_empty()),
                label: personas::seat_disclaimer(s),
            })
            .collect(),
        depth: opts.mode,
    });

    let mut head = String::new();
    let unit = if seats.iter().any(|s| s.persona.is_some()) { "seats" } else { "heads" };
    let mode = if advisory { "advisory".to_string() } else { format!("deliberation (\u{2264}{rounds} rounds)") };
    head.push_str(&format!("# \u{1fa91} Round Table - {n} {unit} - {mode} - {}\n\n_Question:_ {}\n\n", opts.mode.title(), opts.question));
    let brief = opts.brief.as_deref().filter(|b| !b.trim().is_empty());
    if let Some(b) = brief {
        head.push_str(&format!("## Brief\n\n{}\n\n", b.trim()));
    }
    // Clarify first: only on a table, only with no brief and no earlier turns to lean on.
    let fresh = brief.is_none() && opts.prior.as_deref().is_none_or(|p| p.trim().is_empty());
    let check = opts.clarify && n >= 2 && fresh;
    // A one-round Quick run asks the chair alongside the seats, so nobody waits for it; then
    // only the verdict reads the open questions. Any other run asks first and every seat reads them.
    let alongside = check && opts.mode == Mode::Quick && advisory;
    let clarify = || -> Vec<String> {
        let q = crate::coach::clarify(&opts.question, keys, settings, opts.timeout.min(crate::coach::CHAIR_WAIT));
        if !q.is_empty() {
            emit(&Event::Clarify { questions: q.clone() });
        }
        q
    };
    let mut open: Vec<String> = if check && !alongside { clarify() } else { Vec::new() };
    let late_open: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let base = prompt_with_open(&opts.question, opts.prior.as_deref(), opts.context.as_deref(), opts.attachments.as_deref(), opts.brief.as_deref(), open_questions_section(&open).as_deref());

    let mut out = String::new();
    let mut say = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    let first_round = |prompt: &str| {
        std::thread::scope(|s| {
            let asking = alongside.then(|| s.spawn(|| *late_open.lock().unwrap_or_else(|e| e.into_inner()) = clarify()));
            let res = if opts.open_first_round {
                run_open_round(prompt, &seats, &seat_keys, opts, keys, settings, emit)
            } else {
                run_round(prompt, 1, &seats, &seat_keys, opts, keys, settings, emit)
            };
            if let Some(h) = asking {
                let _ = h.join();
            }
            res
        })
    };
    // (label, final answer) per seat, for the verdict.
    let mut finals: Vec<(String, String)> = Vec::new();
    let mut answered = 0usize;
    let mut consensus = None;
    let mut rounds_run = 1;
    let labels_of = |res: &[HeadRun]| -> Vec<String> { seats.iter().zip(res).map(|(s, r)| personas::seat_label(s, &label_for(s.head, settings, r))).collect() };
    // The chair's last word (final verdict, else last round's), for the decision model hook.
    let mut final_chair: Option<String> = None;
    let footer: String;

    if advisory {
        let res = first_round(&base);
        emit(&Event::RoundDone { round: 1 });
        let labels = labels_of(&res);
        finals = labels.iter().cloned().zip(res.iter().map(|r| r.outcome.body())).collect();
        let res: Vec<Outcome> = res.into_iter().map(|r| r.outcome).collect();
        for ((label, o), seat) in labels.iter().zip(&res).zip(&seats) {
            if matches!(o, Outcome::Answered(_)) {
                answered += 1;
            }
            say("---");
            say(&format!("## {label}"));
            say("");
            say(&seat_body(seat, &o.body()));
            say("");
        }
        footer = "_Advisory mode: independent answers, no cross-talk. Use `--rounds N` for deliberation to consensus._".into();
    } else {
        let mut labels: Vec<String> = Vec::new();
        let mut prev: Vec<(String, String)> = Vec::new();
        let mut last_chair: Option<String> = None;
        let mut cons = false;
        for r in 1..=rounds {
            rounds_run = r;
            let prompt = if r == 1 { base.clone() } else { deliberation_prompt(&base, r - 1, &prev, r, last_chair.as_deref()) };
            let res = if r == 1 { first_round(&prompt) } else { run_round(&prompt, r, &seats, &seat_keys, opts, keys, settings, emit) };
            emit(&Event::RoundDone { round: r });
            if r == 1 {
                labels = labels_of(&res);
            }
            let res: Vec<Outcome> = res.into_iter().map(|x| x.outcome).collect();
            answered = answered.max(res.iter().filter(|o| matches!(o, Outcome::Answered(_))).count());
            prev = labels.iter().cloned().zip(res.iter().map(|o| o.body())).collect();
            finals = prev.clone();
            say(&format!("## \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500} Round {r} \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}"));
            say("");
            for ((label, body), seat) in prev.iter().zip(&seats) {
                say(&format!("### {label}"));
                say("");
                say(&seat_body(seat, body));
                say("");
            }
            let reply = chair(keys, settings, &chair_prompt(&opts.question, r, &prev), opts.timeout);
            last_chair = Some(reply.clone()).filter(|c| !c.trim().is_empty());
            let chair = if reply.is_empty() { "VERDICT: CONTINUE\n_(chair unavailable: ANTHROPIC_API_KEY missing or error)_".to_string() } else { reply };
            // Early stop only on true agreement: the same pick, numbers, thresholds and first step.
            let c = agreed(&chair);
            final_chair = Some(chair.clone());
            emit(&Event::ChairVerdict { round: r, consensus: c, text: chair.clone() });
            say(&format!("### \u{1fa91} Chair verdict - round {r}"));
            say("");
            say(&chair);
            say("");
            if c {
                cons = true;
                say(&format!("**\u{2192} Consensus reached at round {r}.**"));
                say("");
                break;
            }
        }
        footer = if cons {
            "_Deliberation converged. The chair's final verdict above is the council consensus._".into()
        } else {
            format!("_Reached the round cap ({rounds}) without full consensus. The chair's last verdict lists the remaining disagreements._")
        };
        consensus = Some(cons);
    }
    if alongside {
        open = late_open.lock().unwrap_or_else(|e| e.into_inner()).clone();
    }
    let transcript = match verdict_section(opts, keys, settings, &open, &finals, emit) {
        // The verdict first; the seat answers folded under it.
        Some((v, text)) => {
            final_chair = Some(text);
            format!("{head}---\n{v}\n---\n{}\n\n{out}</details>\n\n---\n{footer}\n", fold_summary(n, rounds_run))
        }
        None => format!("{head}{out}---\n{footer}\n"),
    };
    let mut transcript = transcript;
    // Decision model hook (decision/HOOK.md): rates the verdict after the final round and the final verdict.
    let decision = crate::decision::after_verdict(
        &crate::decision::Verdict { question: &opts.question, context: opts.context.as_deref(), positions: &finals, chair: final_chair.as_deref(), consensus },
        keys,
        settings,
        opts.timeout,
    );
    if let Some(d) = &decision {
        transcript.push('\n');
        transcript.push_str(&d.markdown());
        emit(&Event::Decision(d.clone()));
    }
    RunResult { transcript, answered, consensus, decision }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::MemoryStore;
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;

    #[test]
    fn consensus_parsing() {
        assert!(is_consensus("VERDICT: CONSENSUS\n- a"));
        assert!(is_consensus("verdict:   consensus"));
        assert!(is_consensus("**VERDICT: CONSENSUS**"));
        assert!(!is_consensus("VERDICT: CONTINUE"));
        assert!(!is_consensus("intro\nVERDICT: CONSENSUS"));
    }

    #[test]
    fn heads_parsing() {
        let (h, w) = parse_heads("claude, gemini,bogus,claude");
        assert_eq!(h.iter().map(|h| h.id).collect::<Vec<_>>(), vec!["claude", "gemini"]);
        assert_eq!(w, vec!["WARN: unknown head 'bogus'"]);
    }

    #[test]
    fn context_prompt() {
        assert_eq!(base_prompt("Q", Some("ctx\n\n")), "## CONTEXT\nctx\n\n## QUESTION\nQ");
        assert_eq!(base_prompt("Q", None), "Q");
    }

    #[test]
    fn default_heads_prefers_settings_then_keys() {
        let store = MemoryStore::default();
        let mut s = Settings::default();
        s.default_heads = vec!["gemini".into()];
        let k = Keys { store: &store, env: BTreeMap::new(), file: BTreeMap::new() };
        assert_eq!(default_heads(&s, &k)[0].id, "gemini");
        let k = Keys {
            store: &store,
            env: [("ROUNDTABLE_HEADS".to_string(), "glm,openai".to_string())].into(),
            file: BTreeMap::new(),
        };
        assert_eq!(default_heads(&Settings::default(), &k).iter().map(|h| h.id).collect::<Vec<_>>(), vec!["glm", "openai"]);
    }

    /// Tiny fake provider: answers every chat call with a fixed text, or a status.
    fn fake_server(status: u16, body: &'static str) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in l.incoming().flatten() {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0usize;
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut buf = vec![0; len];
                let _ = r.read_exact(&mut buf);
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        format!("http://{addr}/v1/chat/completions")
    }

    /// No tool installed on the test machine may answer for a head here.
    fn no_tools() {
        std::env::set_var("ROUNDTABLE_TOOL_PATH", "");
    }

    #[test]
    fn missing_says_what_to_do() {
        let m = |h: &str, r| missing(providers::find(h).unwrap(), r).message().unwrap();
        assert_eq!(m("claude", Route::Auto), "Add a Claude key or install Claude Code.");
        assert_eq!(m("glm", Route::Auto), "Add a GLM key.");
        assert_eq!(m("gemini", Route::Tool), "Gemini CLI is not on this computer.");
        assert_eq!(m("gemini", Route::Key), "Add a Gemini key.");
        assert_eq!(m("codex", Route::Auto), "Install Codex to seat Codex.");
        assert_eq!(missing(providers::find("claude").unwrap(), Route::Auto).fix(), Some(Fix::AddKey));
    }

    #[test]
    fn advisory_run_against_fake_provider() {
        let _env = crate::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        no_tools();
        let url = fake_server(200, r#"{"choices":[{"message":{"content":"Bake bread."}}]}"#);
        std::env::set_var("ROUNDTABLE_GLM_URL", &url);
        let store = MemoryStore::default();
        let k = Keys {
            store: &store,
            env: [("ZAI_API_KEY".to_string(), "test".to_string())].into(),
            file: BTreeMap::new(),
        };
        let (heads, _) = parse_heads("glm,minimax");
        let opts = AskOptions {
            question: "Bread or garden?".into(),
            context: None,
            heads,
            seats: Vec::new(),
            rounds: 1,
            research: false,
            timeout: Duration::from_secs(10),
            ..Default::default()
        };
        let events = Mutex::new(Vec::new());
        let r = run(&opts, &k, &Settings::default(), &|e| events.lock().unwrap().push(serde_json::to_value(e).unwrap()));
        std::env::remove_var("ROUNDTABLE_GLM_URL");
        assert_eq!(r.answered, 1);
        assert!(r.transcript.starts_with("# \u{1fa91} Round Table - 2 heads - advisory - Quick\n"));
        assert!(r.transcript.contains("## \u{1f7e3} GLM (glm-5.2, Z.AI)\n\nBake bread.\n"));
        assert!(r.transcript.contains("_(unavailable, skipped)_"));
        assert!(!r.transcript.contains('\u{2014}'), "no em dashes");
        let ev = events.lock().unwrap();
        assert_eq!(ev.first().unwrap()["event"], "run_started");
        let done: Vec<_> = ev.iter().filter(|e| e["event"] == "head_done").collect();
        assert_eq!(done.len(), 2);
        assert!(done.iter().any(|e| e["head"] == "minimax" && e["status"] == "skipped"));
    }

    #[test]
    fn bad_key_is_a_plain_failure() {
        no_tools();
        let url = fake_server(401, r#"{"error":"invalid key"}"#);
        std::env::set_var("ROUNDTABLE_MINIMAX_URL", &url);
        let store = MemoryStore::default();
        let k = Keys { store: &store, env: [("MINIMAX_API_KEY".to_string(), "x".to_string())].into(), file: BTreeMap::new() };
        let opts = AskOptions {
            question: "Q".into(),
            context: None,
            heads: parse_heads("minimax").0,
            seats: Vec::new(),
            rounds: 2,
            research: false,
            timeout: Duration::from_secs(10),
            ..Default::default()
        };
        let r = run(&opts, &k, &Settings::default(), &|_| {});
        std::env::remove_var("ROUNDTABLE_MINIMAX_URL");
        assert_eq!(r.answered, 0);
        assert_eq!(r.consensus, Some(false));
        assert!(r.transcript.contains("That key did not work"));
        assert!(r.transcript.contains("chair unavailable"));
        assert!(r.transcript.contains("Round 2"));
    }

    #[test]
    fn persona_run_labels_seats_and_caps_rounds() {
        no_tools();
        let url = fake_server(200, r#"{"choices":[{"message":{"content":"Hold prices."}}]}"#);
        std::env::set_var("ROUNDTABLE_GROK_XAI_URL", &url);
        let store = MemoryStore::default();
        let k = Keys { store: &store, env: [("XAI_API_KEY".to_string(), "test".to_string())].into(), file: BTreeMap::new() };
        let heads = parse_heads("grok").0;
        let lib = personas::Library::from_packs(personas::builtin_packs(), false);
        let spec = if personas::lab_pack_built() { "cfo,nvidia-view" } else { "cfo,ceo" };
        let seats = personas::resolve(spec, &heads, &lib).unwrap();
        let opts = AskOptions {
            question: "Raise prices?".into(),
            context: None,
            heads,
            seats,
            rounds: 9,
            research: false,
            timeout: Duration::from_secs(10),
            ..Default::default()
        };
        let events = Mutex::new(Vec::new());
        let r = run(&opts, &k, &Settings::default(), &|e| events.lock().unwrap().push(serde_json::to_value(e).unwrap()));
        std::env::remove_var("ROUNDTABLE_GROK_XAI_URL");
        assert!(r.transcript.starts_with("# \u{1fa91} Round Table - 2 seats - deliberation (\u{2264}4 rounds) - Quick\n"), "{}", r.transcript);
        assert!(r.transcript.contains("### CFO \u{b7} \u{1f916} Grok"), "{}", r.transcript);
        assert!(r.transcript.contains("Round 4"));
        assert!(!r.transcript.contains("Round 5"));
        let ev = events.lock().unwrap();
        let start = &ev[0];
        assert_eq!(start["rounds"], 4);
        assert_eq!(start["seats"][0]["seat"], "cfo@grok");
        assert_eq!(start["seats"][0]["persona"], "CFO");
        if personas::lab_pack_built() {
            assert_eq!(start["seats"][1]["label"], "Simulated perspective drawn from public positions. Not affiliated with or endorsed by NVIDIA.");
            assert!(r.transcript.contains("_Simulated perspective drawn from public positions. Not affiliated with or endorsed by NVIDIA._\n\nHold prices."));
        }
        let done: Vec<_> = ev.iter().filter(|e| e["event"] == "head_done").collect();
        assert_eq!(done.len(), 8, "two seats, four rounds");
        assert!(done.iter().all(|e| e["seat"] != e["head"]));
    }

    /// A fake that reads the prompt: OpenAI-style seats say whether they saw earlier answers;
    /// the Anthropic-style chair answers verdict prompts with a vote naming a minority.
    fn reading_server() -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in l.incoming().flatten() {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0usize;
                let mut path = String::new();
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if path.is_empty() {
                        path = line.clone();
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut buf = vec![0; len];
                let _ = r.read_exact(&mut buf);
                let v: serde_json::Value = serde_json::from_slice(&buf).unwrap_or_default();
                let prompt = v["messages"][0]["content"].as_str().unwrap_or("").to_string();
                let body = if path.contains("/v1/messages") {
                    let t = if prompt.contains("Write the final verdict") {
                        format!("VOTES:\n- a: garden\nRESULT: garden wins 1 to 0\nMINORITY VIEW: Bread first.\nHELD BY: {}", if prompt.contains("THE BRIEF:") { "the brief reader" } else { "Nobody" })
                    } else {
                        "VERDICT: CONSENSUS\n**AGREED**\n- garden".to_string()
                    };
                    serde_json::json!({"content": [{"type": "text", "text": t}]})
                } else {
                    let saw = prompt.contains("ANSWERS SO FAR");
                    let brief = prompt.contains("## BRIEF\n- **Goal:** g");
                    serde_json::json!({"choices": [{"message": {"content": format!("saw={saw} brief={brief}")}}]})
                }
                .to_string();
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn open_round_brief_and_verdict() {
        let _env = crate::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        no_tools();
        let base = reading_server();
        std::env::set_var("ROUNDTABLE_OPENAI_URL", format!("{base}/v1/chat/completions"));
        std::env::set_var("ROUNDTABLE_ANTHROPIC_URL", format!("{base}/v1/messages"));
        let store = MemoryStore::default();
        let k = Keys {
            store: &store,
            env: [("OPENAI_API_KEY".to_string(), "t".to_string()), ("ANTHROPIC_API_KEY".to_string(), "t".to_string())].into(),
            file: BTreeMap::new(),
        };
        let heads = parse_heads("openai").0;
        let lib = personas::Library::from_packs(personas::builtin_packs(), true);
        let opts = AskOptions {
            question: "Bread or garden?".into(),
            seats: personas::resolve("cfo,ceo", &heads, &lib).unwrap(),
            heads,
            rounds: 1,
            timeout: Duration::from_secs(10),
            brief: Some("- **Goal:** g".into()),
            open_first_round: true,
            verdict: Some(Shape::Vote),
            ..Default::default()
        };
        let events = Mutex::new(Vec::new());
        let r = run(&opts, &k, &Settings::default(), &|e| events.lock().unwrap().push(serde_json::to_value(e).unwrap()));
        std::env::remove_var("ROUNDTABLE_OPENAI_URL");
        std::env::remove_var("ROUNDTABLE_ANTHROPIC_URL");
        let t = &r.transcript;
        assert!(t.contains("_Question:_ Bread or garden?\n\n## Brief\n\n- **Goal:** g\n"), "{t}");
        // The first seat answers blind; the second reads the first.
        let a = t.find("saw=false brief=true").expect(t);
        let b = t.find("saw=true brief=true").expect(t);
        assert!(a < b);
        assert!(t.contains("## \u{1fa91} Verdict (vote)\n\nVOTES:"), "{t}");
        assert!(t.contains("HELD BY: the brief reader"));
        let ev = events.lock().unwrap();
        let v = ev.iter().find(|e| e["event"] == "verdict").expect("verdict event");
        assert_eq!(v["shape"], "vote");
        assert_eq!(v["minority"], "Bread first.");
        assert_eq!(v["held_by"], "the brief reader");
        // Blind (the default) and no verdict: the old transcript.
        let blind = AskOptions { open_first_round: false, verdict: None, brief: None, ..opts.clone() };
        std::env::set_var("ROUNDTABLE_OPENAI_URL", format!("{base}/v1/chat/completions"));
        let r2 = run(&blind, &k, &Settings::default(), &|_| {});
        std::env::remove_var("ROUNDTABLE_OPENAI_URL");
        assert!(!r2.transcript.contains("saw=true"));
        assert!(!r2.transcript.contains("Verdict"));
        assert!(!r2.transcript.contains("## Brief"));
    }

    #[test]
    fn early_stop_only_on_true_agreement() {
        let c = "VERDICT: CONSENSUS\nLEADING PICK: Postgres.\n**AGREED**\n- Postgres\n\n**DIFFERENCES**\n- none\n";
        assert!(agreed(c));
        let c = "VERDICT: CONSENSUS\nLEADING PICK: Raise prices.\n**AGREED**\n- raise\n**DIFFERENCES**\n- Claude says 10%, Codex says 15%.\n- First step: Grok emails clients, Claude audits invoices.\n";
        assert!(is_consensus(c) && !agreed(c), "same pick, different numbers: another round");
        assert_eq!(differences(c).len(), 2);
        assert!(differences("VERDICT: CONTINUE\nDIFFERENCES: none").is_empty());
        assert_eq!(differences("**DIFFERENCES:** Price differs.\n- Start date differs.\n\nThen more text"), ["Price differs.", "Start date differs."]);
        assert!(agreed("VERDICT: CONSENSUS\n**AGREED**\n- x"), "no DIFFERENCES heading at all is no listed difference");
        let p = chair_prompt("Q", 1, &[("A".into(), "x".into())]);
        assert!(p.contains("LEADING PICK:") && p.contains("**DIFFERENCES**") && p.contains("Any bullet under DIFFERENCES means CONTINUE."));
        assert!(!p.contains('\u{2014}'));
    }

    #[test]
    fn round_two_argues_against_the_leading_pick() {
        let t = [("A".to_string(), "x".to_string())];
        let p = deliberation_prompt("Q", 1, &t, 2, Some("VERDICT: CONTINUE\nLEADING PICK: P."));
        assert!(p.starts_with("Q\n\n---\n## COUNCIL DELIBERATION: every councilor's position from round 1:\n\n### A\nx\n\n## THE CHAIR'S READING OF ROUND 1\nVERDICT: CONTINUE\nLEADING PICK: P.\n\n## YOUR TASK (round 2)\n"), "{p}");
        assert!(p.contains("argue against the leading pick") && p.contains("Begin your reply with \"CASE AGAINST:\""));
        assert!(!deliberation_prompt("Q", 1, &t, 2, None).contains("CHAIR'S READING"));
        assert!(!p.contains('\u{2014}'));
    }

    /// A chair that asks two questions, keeps finding a difference, and gives a decision verdict.
    fn clarifying_server() -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in l.incoming().flatten() {
                let mut r = BufReader::new(stream.try_clone().unwrap());
                let mut len = 0usize;
                let mut path = String::new();
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if path.is_empty() {
                        path = line.clone();
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut buf = vec![0; len];
                let _ = r.read_exact(&mut buf);
                let v: serde_json::Value = serde_json::from_slice(&buf).unwrap_or_default();
                let prompt = v["messages"][0]["content"].as_str().unwrap_or("").to_string();
                let body = if path.contains("/v1/messages") {
                    let t = if prompt.contains("decide whether the person must answer questions first") {
                        "Q: What is the workload?\nQ: How many users?".to_string()
                    } else if prompt.contains("Write the final verdict") {
                        let open = prompt.contains("OPEN QUESTIONS:\n- What is the workload?") && prompt.contains("QUESTIONS FIRST:");
                        let deep = prompt.contains("**THE MINORITY VIEW IN FULL**");
                        format!("QUESTIONS FIRST:\n1. What is the workload?\n2. How many users?\nDECISION: If reads dominate, Postgres. open={open} deep={deep}\nCONFIDENCE: medium (60%)\nWOULD CHANGE IT: Heavy writes.\nNEXT STEPS:\n1. Count queries.\n2. Price hosts.\n3. Pick.\n**WHY**\n- x\nMINORITY VIEW: Mongo.\nHELD BY: B")
                    } else {
                        "VERDICT: CONSENSUS\nLEADING PICK: Postgres.\n**DIFFERENCES**\n- A says 2 GB, B says 8 GB.".to_string()
                    };
                    serde_json::json!({"content": [{"type": "text", "text": t}]})
                } else {
                    let open = prompt.contains("## OPEN QUESTIONS\nThe chair would ask the person these first.");
                    let reading = prompt.contains("## THE CHAIR'S READING OF ROUND 1\nVERDICT: CONSENSUS");
                    let words = prompt.rsplit("## LENGTH\n").next().filter(|_| prompt.contains("## LENGTH\n")).and_then(|r| r.split("about ").nth(1)).and_then(|r| r.split(' ').next()).unwrap_or("none");
                    serde_json::json!({"choices": [{"message": {"content": format!("open={open} reading={reading} words={words}")}}]})
                }
                .to_string();
                let mut s = stream;
                let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn verdict_first_clarify_first_and_a_forced_second_round() {
        let _env = crate::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        no_tools();
        let base = clarifying_server();
        std::env::set_var("ROUNDTABLE_OPENAI_URL", format!("{base}/v1/chat/completions"));
        std::env::set_var("ROUNDTABLE_GLM_URL", format!("{base}/v1/chat/completions"));
        std::env::set_var("ROUNDTABLE_ANTHROPIC_URL", format!("{base}/v1/messages"));
        let store = MemoryStore::default();
        let k = Keys {
            store: &store,
            env: [("OPENAI_API_KEY".to_string(), "t".to_string()), ("ZAI_API_KEY".to_string(), "t".to_string()), ("ANTHROPIC_API_KEY".to_string(), "t".to_string())].into(),
            file: BTreeMap::new(),
        };
        let opts = AskOptions {
            question: "Which database?".into(),
            heads: parse_heads("openai,glm").0,
            rounds: 2,
            timeout: Duration::from_secs(10),
            verdict: Some(Shape::Decision),
            clarify: true,
            ..Default::default()
        };
        let events = Mutex::new(Vec::new());
        let r = run(&opts, &k, &Settings::default(), &|e| events.lock().unwrap().push(serde_json::to_value(e).unwrap()));
        std::env::remove_var("ROUNDTABLE_OPENAI_URL");
        std::env::remove_var("ROUNDTABLE_GLM_URL");
        std::env::remove_var("ROUNDTABLE_ANTHROPIC_URL");
        let t = &r.transcript;
        // The verdict leads; the seat answers sit folded under it.
        let v = t.find("## \u{1fa91} Verdict (decision)\n\nQUESTIONS FIRST:\n1. What is the workload?").expect(t);
        let fold = t.find("<details><summary>The seat answers and the rounds (2 seats, 2 rounds)</summary>").expect(t);
        let r1 = t.find("Round 1").expect(t);
        assert!(v < fold && fold < r1, "{t}");
        assert!(t.contains("open=true") && t.contains("DECISION: If reads dominate, Postgres. open=true"), "seats and the verdict read the open questions: {t}");
        // The chair said CONSENSUS but listed a difference: round 2 ran, and read the chair.
        assert!(t.contains("Round 2") && t.contains("open=true reading=true"), "{t}");
        assert_eq!(r.consensus, Some(false));
        assert!(t.trim_end().ends_with("without full consensus. The chair's last verdict lists the remaining disagreements._"));
        assert!(t.contains("</details>\n\n---\n_Reached the round cap"));
        let ev = events.lock().unwrap();
        let c = ev.iter().find(|e| e["event"] == "clarify").expect("clarify event");
        assert_eq!(c["questions"][1], "How many users?");
        let v = ev.iter().find(|e| e["event"] == "verdict").unwrap();
        assert_eq!(v["next_steps"].as_array().unwrap().len(), 3);
        assert_eq!(v["would_change"], "Heavy writes.");
        assert_eq!(v["questions"].as_array().unwrap().len(), 2);
        assert!(ev.iter().filter(|e| e["event"] == "chair_verdict").all(|e| e["consensus"] == false));
        // One AI on its own: no gate, no verdict, the old transcript.
        let solo = AskOptions { heads: parse_heads("openai").0, rounds: 1, verdict: None, ..opts.clone() };
        std::env::set_var("ROUNDTABLE_OPENAI_URL", format!("{base}/v1/chat/completions"));
        let r = run(&solo, &k, &Settings::default(), &|_| {});
        std::env::remove_var("ROUNDTABLE_OPENAI_URL");
        assert!(r.transcript.contains("open=false") && !r.transcript.contains("<details>") && !r.transcript.contains("Verdict"), "{}", r.transcript);
    }

    fn clarifying_keys(base: &str) -> BTreeMap<String, String> {
        std::env::set_var("ROUNDTABLE_OPENAI_URL", format!("{base}/v1/chat/completions"));
        std::env::set_var("ROUNDTABLE_GLM_URL", format!("{base}/v1/chat/completions"));
        std::env::set_var("ROUNDTABLE_ANTHROPIC_URL", format!("{base}/v1/messages"));
        [("OPENAI_API_KEY".to_string(), "t".to_string()), ("ZAI_API_KEY".to_string(), "t".to_string()), ("ANTHROPIC_API_KEY".to_string(), "t".to_string())].into()
    }

    fn clear_urls() {
        for k in ["ROUNDTABLE_OPENAI_URL", "ROUNDTABLE_GLM_URL", "ROUNDTABLE_ANTHROPIC_URL"] {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn quick_asks_the_chair_alongside_and_keeps_answers_short() {
        let _env = crate::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        no_tools();
        let base = clarifying_server();
        let store = MemoryStore::default();
        let k = Keys { store: &store, env: clarifying_keys(&base), file: BTreeMap::new() };
        let opts = AskOptions {
            question: "Which database?".into(),
            heads: parse_heads("openai,glm").0,
            rounds: 1,
            timeout: Duration::from_secs(10),
            verdict: Some(Shape::Decision),
            clarify: true,
            mode: Mode::Quick,
            ..Default::default()
        };
        let events = Mutex::new(Vec::new());
        let r = run(&opts, &k, &Settings::default(), &|e| events.lock().unwrap().push(serde_json::to_value(e).unwrap()));
        clear_urls();
        let t = &r.transcript;
        assert!(t.starts_with("# \u{1fa91} Round Table - 2 heads - advisory - Quick\n"), "{t}");
        // The seats did not wait for the open questions; the verdict read them, in the short shape.
        assert!(t.contains("open=false reading=false words=250"), "{t}");
        assert!(t.contains("DECISION: If reads dominate, Postgres. open=true deep=false"), "{t}");
        let ev = events.lock().unwrap();
        assert_eq!(ev[0]["event"], "run_started");
        assert_eq!(ev[0]["depth"], "quick");
        assert!(ev.iter().any(|e| e["event"] == "clarify"));
        assert!(!ev.iter().any(|e| e["event"] == "chair_verdict"), "one round has no chair ruling");
    }

    #[test]
    fn deep_think_asks_first_writes_long_and_deliberates() {
        let _env = crate::TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        no_tools();
        let base = clarifying_server();
        let store = MemoryStore::default();
        let k = Keys { store: &store, env: clarifying_keys(&base), file: BTreeMap::new() };
        let opts = AskOptions {
            question: "Which database?".into(),
            heads: parse_heads("openai,glm").0,
            rounds: 2,
            timeout: Duration::from_secs(10),
            verdict: Some(Shape::Decision),
            clarify: true,
            mode: Mode::Deep,
            ..Default::default()
        };
        let events = Mutex::new(Vec::new());
        let r = run(&opts, &k, &Settings::default(), &|e| events.lock().unwrap().push(serde_json::to_value(e).unwrap()));
        clear_urls();
        let t = &r.transcript;
        assert!(t.starts_with("# \u{1fa91} Round Table - 2 heads - deliberation (\u{2264}2 rounds) - Deep Think\n"), "{t}");
        assert!(t.contains("open=true reading=false words=800") && t.contains("open=true reading=true words=800"), "{t}");
        assert!(t.contains("DECISION: If reads dominate, Postgres. open=true deep=true"), "{t}");
        assert_eq!(events.lock().unwrap()[0]["depth"], "deep");
    }

    #[test]
    fn prompt_sections_in_order() {
        assert_eq!(prompt_with("Q", Some("p"), Some("c"), Some("a"), Some("b")), "## EARLIER IN THIS SESSION\np\n\n## CONTEXT\nc\n\n## ATTACHMENTS\na\n\n## BRIEF\nb\n\n## QUESTION\nQ");
        assert_eq!(prompt_with("Q", None, None, None, Some("  ")), "Q");
        assert_eq!(open_round_prompt("B", &[]), "B");
        assert!(open_round_prompt("B", &[("L".into(), "x".into())]).starts_with("B\n\n---\n## ANSWERS SO FAR (round 1 is open)"));
    }
}
