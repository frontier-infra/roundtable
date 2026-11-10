//! The council: same flow and transcript as `lib/core.sh`.
//!
//! Advisory (`rounds <= 1`): each head answers once, blind.
//! Deliberation (`rounds >= 2`): round 1 blind; later rounds see the whole table;
//! a Claude chair says CONSENSUS or CONTINUE after each round and the run stops early on consensus.

use crate::clis::{self, Fix};
use crate::http::{self, ErrorClass};
use crate::keys::Keys;
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
    pub heads: Vec<&'static Head>,
    /// Who sits where. Empty means one Generic seat per head (the old table).
    pub seats: Vec<Seat>,
    pub rounds: u32,
    pub research: bool,
    pub timeout: Duration,
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
    RunStarted { heads: Vec<String>, rounds: u32, mode: String, seats: Vec<SeatInfo> },
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
        match HEADS.iter().find(|h| h.id == name) {
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
    let ready: Vec<_> = HEADS
        .iter()
        .filter(|h| match h.kind {
            // codex sits by default when installed, as in the bash engine; other tools when chosen.
            Kind::Cli => (h.id == "codex" || chosen(h)) && cli_found(h),
            _ => keys.key(h).is_some() || ((chosen(h) || h.id == "grok") && cli_found(h)),
        })
        .collect();
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
    let mut parts: Vec<String> = Vec::new();
    for (title, v) in [("EARLIER IN THIS SESSION", prior), ("CONTEXT", context), ("ATTACHMENTS", attachments), ("BRIEF", brief)] {
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

fn deliberation_prompt(base: &str, prev: u32, table: &[(String, String)], round: u32) -> String {
    let mut p = format!("{base}\n\n---\n## COUNCIL DELIBERATION: every councilor's position from round {prev}:\n\n");
    for (label, body) in table {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    p.push_str(&format!(
        "## YOUR TASK (round {round})\nYou are ONE councilor. You have now read everyone. Update your position. State explicitly: (a) where a peer changed your mind; name them; (b) where you still disagree and why; (c) your sharpened current answer. Begin your reply with \"POSITION:\".\n"
    ));
    p
}

fn chair_prompt(question: &str, round: u32, table: &[(String, String)]) -> String {
    let mut p = format!(
        "You are the neutral CHAIR of an AI design council. The councilors submitted these round {round} positions to this question:\n\n{question}\n\n--- POSITIONS ---\n\n"
    );
    for (label, body) in table {
        p.push_str(&format!("### {label}\n{body}\n\n"));
    }
    p.push_str("--- YOUR JOB ---\nLine 1 must be EXACTLY one of: \"VERDICT: CONSENSUS\" (the councilors substantively agree on the core recommendation) or \"VERDICT: CONTINUE\" (material disagreement remains). Then give: **AGREED** (bullets) and **REMAINING DISAGREEMENTS** (bullets; name who holds what). Be strict: consensus means real convergence on the answer, not mere overlap.\n");
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
    }
}

/// Ask one head by the route that works: the person's own signed-in tool first,
/// the API key second. Skipped only when it has neither.
/// `key_model` overrides the key's model (the chair asks Claude with its own model).
#[allow(clippy::too_many_arguments)]
pub fn ask_head(head: &Head, prompt: &str, research: bool, timeout: Duration, keys: &Keys, settings: &Settings, route: Route, key_model: Option<&str>) -> HeadRun {
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
pub fn chair(keys: &Keys, settings: &Settings, prompt: &str, timeout: Duration) -> String {
    let claude = providers::find("claude").unwrap();
    match ask_head(claude, prompt, false, timeout, keys, settings, Route::Auto, Some(&providers::chair_model(settings))).outcome {
        Outcome::Answered(t) => t,
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
                    let p = personas::seat_prompt(seat, prompt);
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
        let p = personas::seat_prompt(seat, &open_round_prompt(base, &before));
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

/// The verdict section, when the run asked for one. Returns the verdict text.
fn say_verdict(say: &mut dyn FnMut(&str), opts: &AskOptions, keys: &Keys, settings: &Settings, finals: &[(String, String)], emit: &(dyn Fn(&Event) + Sync)) -> Option<String> {
    let shape = opts.verdict?;
    let text = chair(keys, settings, &verdict::prompt(shape, &opts.question, opts.brief.as_deref(), finals), opts.timeout);
    let text = if text.trim().is_empty() { verdict::UNAVAILABLE.to_string() } else { text.trim().to_string() };
    emit(&Event::Verdict { shape, parts: verdict::parts(&text), text: text.clone() });
    say("---");
    say(&format!("## \u{1fa91} Verdict ({})", shape.title().to_ascii_lowercase()));
    say("");
    say(&text);
    say("");
    Some(text)
}

/// Run the council. `emit` gets every event as it happens; the transcript comes back.
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
    });

    let mut out = String::new();
    let mut say = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    let unit = if seats.iter().any(|s| s.persona.is_some()) { "seats" } else { "heads" };
    let mode = if advisory { "advisory".to_string() } else { format!("deliberation (\u{2264}{rounds} rounds)") };
    say(&format!("# \u{1fa91} Round Table - {n} {unit} - {mode}"));
    say("");
    say(&format!("_Question:_ {}", opts.question));
    say("");

    if let Some(b) = opts.brief.as_deref().filter(|b| !b.trim().is_empty()) {
        say("## Brief");
        say("");
        say(b.trim());
        say("");
    }
    let base = prompt_with(&opts.question, opts.prior.as_deref(), opts.context.as_deref(), opts.attachments.as_deref(), opts.brief.as_deref());
    let first_round = |prompt: &str| {
        if opts.open_first_round {
            run_open_round(prompt, &seats, &seat_keys, opts, keys, settings, emit)
        } else {
            run_round(prompt, 1, &seats, &seat_keys, opts, keys, settings, emit)
        }
    };
    // (label, final answer) per seat, for the verdict.
    let mut finals: Vec<(String, String)> = Vec::new();
    let mut answered = 0usize;
    let mut consensus = None;
    let labels_of = |res: &[HeadRun]| -> Vec<String> { seats.iter().zip(res).map(|(s, r)| personas::seat_label(s, &label_for(s.head, settings, r))).collect() };
    // The chair's last word (final verdict, else last round's), for the decision model hook.
    let mut final_chair: Option<String> = None;

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
        final_chair = say_verdict(&mut say, opts, keys, settings, &finals, emit);
        say("---");
        say("_Advisory mode: independent answers, no cross-talk. Use `--rounds N` for deliberation to consensus._");
    } else {
        let mut labels: Vec<String> = Vec::new();
        let mut prev: Vec<(String, String)> = Vec::new();
        let mut cons = false;
        for r in 1..=rounds {
            let prompt = if r == 1 { base.clone() } else { deliberation_prompt(&base, r - 1, &prev, r) };
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
            let chair = chair(keys, settings, &chair_prompt(&opts.question, r, &prev), opts.timeout);
            let chair = if chair.is_empty() {
                "VERDICT: CONTINUE\n_(chair unavailable: ANTHROPIC_API_KEY missing or error)_".to_string()
            } else {
                chair
            };
            let c = is_consensus(&chair);
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
        if let Some(v) = say_verdict(&mut say, opts, keys, settings, &finals, emit) {
            final_chair = Some(v);
        }
        say("---");
        if cons {
            say("_Deliberation converged. The chair's final verdict above is the council consensus._");
        } else {
            say(&format!("_Reached the round cap ({rounds}) without full consensus. The chair's last verdict lists the remaining disagreements._"));
        }
        consensus = Some(cons);
    }
    // Decision model hook (decision/HOOK.md): rates the verdict after the final round and the final verdict.
    let decision = crate::decision::after_verdict(
        &crate::decision::Verdict { question: &opts.question, context: opts.context.as_deref(), positions: &finals, chair: final_chair.as_deref(), consensus },
        keys,
        settings,
        opts.timeout,
    );
    if let Some(d) = &decision {
        out.push('\n');
        out.push_str(&d.markdown());
        emit(&Event::Decision(d.clone()));
    }
    RunResult { transcript: out, answered, consensus, decision }
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
        assert!(r.transcript.starts_with("# \u{1fa91} Round Table - 2 heads - advisory\n"));
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
        assert!(r.transcript.starts_with("# \u{1fa91} Round Table - 2 seats - deliberation (\u{2264}4 rounds)\n"), "{}", r.transcript);
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
    fn prompt_sections_in_order() {
        assert_eq!(prompt_with("Q", Some("p"), Some("c"), Some("a"), Some("b")), "## EARLIER IN THIS SESSION\np\n\n## CONTEXT\nc\n\n## ATTACHMENTS\na\n\n## BRIEF\nb\n\n## QUESTION\nQ");
        assert_eq!(prompt_with("Q", None, None, None, Some("  ")), "Q");
        assert_eq!(open_round_prompt("B", &[]), "B");
        assert!(open_round_prompt("B", &[("L".into(), "x".into())]).starts_with("B\n\n---\n## ANSWERS SO FAR (round 1 is open)"));
    }
}
