// Pure state for the walkthrough and the live roundtable. No DOM here, so it is easy to test.

import type { ClaudeStatus, CliDetection, EngineEvent, Fix, HeadId, HeadTest, HistoryEntry, KeySaveResult, KeysStatus, Persona, PersonaLibrary, Preset, SeatInfo, Settings } from "./api";
import { HEAD_ORDER, PROVIDERS } from "./providers";

export const STEPS = 5;
export type Step = 1 | 2 | 3 | 4 | 5;

export type KeyCardStatus = "empty" | "checking" | "connected" | "error";

export interface KeyCard {
  status: KeyCardStatus;
  message?: string;
  /** Where the saved key came from. "file" and "env" mean the command-line version set it. */
  source?: string | null;
}

export type SetupStatus = "idle" | "working" | "done" | "failed";

export interface SetupState {
  status: SetupStatus;
  /** How many of the three ticks are green. */
  ticks: number;
  attempts: number;
  error?: { code: string; detail: string };
}

export type CardStatus = "waiting" | "thinking" | "answered" | "skipped" | "timed_out" | "failed";

export interface HeadCard {
  head: string;
  status: CardStatus;
  text: string;
  /** Why it did not answer, in plain words. */
  message?: string;
  /** The one thing to do about it. */
  fix?: Fix;
  /** How it was asked: the tool's name, or "API key". */
  via?: string;
  round: number;
  /** The seat key: the head id, or `<persona>@<head>`. */
  seat?: string;
  /** The persona sitting on this AI. */
  persona?: string;
  expertise?: string;
  /** The lab label on a lab seat. */
  label?: string;
}

/** One round of a run: every seat's answer in that round, and the chair's verdict. */
export interface RoundStep {
  round: number;
  answers: Record<string, { status: CardStatus; text: string }>;
  verdict?: Verdict;
}

/** A five-word test of one AI (setup screen and Settings). */
export interface TestView {
  status: "testing" | "ok" | "failed";
  /** Failed tests in a row. Setup unticks a tool after two. */
  failures: number;
  result?: HeadTest;
  /** Setup unticked it after two failures. */
  unticked?: boolean;
}

/** Setup unticks a tool after this many failed tests in a row. */
export const TEST_TRIES = 2;

export interface Verdict {
  round: number;
  consensus: boolean;
  text: string;
}

export interface RunView {
  question: string;
  mode: "quick" | "talk";
  research?: boolean;
  /** Background for a retry: a file path, or "follow-up". */
  context?: string | null;
  /** Seat keys in table order. On a Generic table these are the head ids. */
  heads: string[];
  seats: Record<string, SeatInfo>;
  steps: RoundStep[];
  cards: Record<string, HeadCard>;
  verdicts: Verdict[];
  round: number;
  rounds: number;
  done: boolean;
  stopped: boolean;
  transcript: string;
  answered: number;
  consensus?: boolean;
}

export interface ClaudeView extends Partial<ClaudeStatus> {
  loading: boolean;
  busy?: "app" | "code" | "plugin";
  appMessage?: string;
  codeMessage?: string;
  pluginMessage?: string;
  error?: string;
}

export interface CliView {
  /** Detection finished at least once. */
  loaded: boolean;
  loading: boolean;
  list: CliDetection[];
  error?: string;
  /** Tool ids whose sign-in window we opened. */
  signingIn: string[];
}

export type View = "onboarding" | "main" | "settings";

export interface State {
  view: View;
  step: Step;
  os: string;
  version: string;
  setup: SetupState;
  keys: Record<HeadId, KeyCard>;
  importable: string[];
  moreOpen: boolean;
  clis: CliView;
  /** The "Or use an API key" section. null: open only when no tool was found. */
  keysOpen: boolean | null;
  claude: ClaudeView;
  firstQuestion: string;
  run: RunView | null;
  ask: AskBox;
  /** Persona packs from the engine. null until loaded. */
  personas: PersonaLibrary | null;
  /** The persona open in the Personas editor. */
  personaEdit: Persona | null;
  history: HistoryEntry[];
  historyFilter: string;
  opened: { id: string; transcript: string } | null;
  settings: Settings;
  /** Five-word tests, by test key (`app:<head>` or `key:<head>`). */
  tests: Record<string, TestView>;
  update: { version: string } | null;
  toast: string | null;
  engineReady: boolean;
}

/** Who sits at the table for one question. */
export interface SeatPick {
  /** Persona id, or "" for a Generic seat. */
  persona: string;
  head: string;
}

export interface AskBox {
  question: string;
  /** 1 to 4. One round is independent answers; more rounds talk it through. */
  rounds: number;
  research: boolean;
  contextPath: string | null;
  contextText: string | null;
  contextName: string | null;
  /** "" (Generic), "preset:<id>", "pack:<id>" or "custom". */
  seats: string;
  /** The seats when `seats` is "custom". */
  seatList: SeatPick[];
}

export const FIRST_QUESTION = "I have a free Saturday. Should I learn to bake bread or start a vegetable garden?";

export function defaultSettings(): Settings {
  return {
    default_heads: [],
    models: {},
    timeout_secs: null,
    history: true,
    auto_update: true,
    onboarded: false,
    terminal_command: false,
    cli_heads: [],
    clis_chosen: false,
    show_lab_pack: true,
    personas: "",
  };
}

export function emptyKeys(): Record<HeadId, KeyCard> {
  const k = {} as Record<HeadId, KeyCard>;
  for (const p of PROVIDERS) k[p.id] = { status: "empty" };
  return k;
}

export function initialState(): State {
  return {
    view: "onboarding",
    step: 1,
    os: "macos",
    version: "",
    setup: { status: "idle", ticks: 0, attempts: 0 },
    keys: emptyKeys(),
    importable: [],
    moreOpen: false,
    clis: { loaded: false, loading: false, list: [], signingIn: [] },
    keysOpen: null,
    claude: { loading: true },
    firstQuestion: FIRST_QUESTION,
    run: null,
    ask: { question: "", rounds: DEFAULT_ROUNDS, research: false, contextPath: null, contextText: null, contextName: null, seats: "", seatList: [] },
    personas: null,
    personaEdit: null,
    history: [],
    historyFilter: "",
    opened: null,
    settings: defaultSettings(),
    tests: {},
    update: null,
    toast: null,
    engineReady: false,
  };
}

// ---- Walkthrough ----

export function next(step: Step): Step {
  return Math.min(STEPS, step + 1) as Step;
}

export function back(step: Step): Step {
  return Math.max(1, step - 1) as Step;
}

/** Screens 3 to 5 have "Skip for now". */
export function canSkip(step: Step): boolean {
  return step >= 3;
}

/** Every head that can answer: a working key, or a ticked tool. In table order. */
export function connectedHeads(keys: Record<string, KeyCard>, cliHeads: string[] = []): string[] {
  const on = new Set<string>(cliHeads);
  for (const p of PROVIDERS) if (keys[p.id]?.status === "connected") on.add(p.id);
  return HEAD_ORDER.filter((h) => on.has(h));
}

/** Tools found and signed in. These start ticked until the person changes a tick. */
export function defaultCliHeads(list: CliDetection[]): string[] {
  const on = new Set(list.filter((d) => d.found && d.signed_in === true).map((d) => d.head));
  return HEAD_ORDER.filter((h) => on.has(h));
}

/** Heads running through a ticked tool that is still on this computer, and
 * whose last test did not fail. Before detection finishes, the saved list stands. */
export function cliHeadsOn(s: Pick<State, "clis" | "settings"> & Partial<Pick<State, "tests">>): string[] {
  const on = s.settings.cli_heads ?? [];
  const broken = (h: string) => (s.tests?.[`app:${h}`]?.failures ?? 0) >= TEST_TRIES;
  if (!s.clis.loaded) return HEAD_ORDER.filter((h) => on.includes(h) && !broken(h));
  const found = new Set(s.clis.list.filter((d) => d.found && d.signed_in !== false).map((d) => d.head));
  return HEAD_ORDER.filter((h) => on.includes(h) && found.has(h) && !broken(h));
}

/** The test line under a tool or key card. */
export function testLine(t: TestView | undefined): string | null {
  if (!t) return null;
  if (t.status === "testing") return t.failures ? "Trying once more..." : "Checking with a five-word question...";
  const r = t.result;
  if (t.status === "ok" && r) return `Works. Answered in ${r.secs} s${r.via ? ` through ${r.via}` : ""}: “${r.first_line ?? ""}”`;
  const why = r?.message ?? "It did not answer.";
  return t.unticked ? `Unticked: ${why}` : why;
}

/** The button for a fix, or null. */
export function fixLabel(fix: Fix | undefined | null, name: string): string | null {
  switch (fix) {
    case "sign_in":
      return `Sign in to ${name}`;
    case "add_key":
      return "Add a key";
    case "install":
      return null;
    case "retry":
      return "Retry";
    default:
      return null;
  }
}

/** The line under a tool's name. Two sentences at most. */
export function cliCardText(d: CliDetection, os: string): string {
  if (d.signed_in === true) return `${d.name}, signed in. ${d.note}`;
  if (d.signed_in === false) return `${d.name} is installed but not signed in. Click Sign in to open it in ${os === "windows" ? "a terminal window" : "Terminal"}.`;
  return `${d.name} is installed. We could not tell if you are signed in.`;
}

type ContinueState = Pick<State, "step" | "setup" | "keys"> & Partial<Pick<State, "clis" | "settings" | "tests">>;

function cliHeadsOf(s: Partial<Pick<State, "clis" | "settings" | "tests">>): string[] {
  return s.clis && s.settings ? cliHeadsOn({ clis: s.clis, settings: s.settings, tests: s.tests }) : [];
}

/** Continue on screen 2 needs a finished setup; on screen 3, one AI: a ticked tool or a key. */
export function canContinue(s: ContinueState): boolean {
  if (s.step === 2) return s.setup.status === "done";
  if (s.step === 3) return connectedHeads(s.keys, cliHeadsOf(s)).length > 0;
  return true;
}

export function keychainName(os: string): string {
  return os === "windows" ? "Windows Credential Manager" : "your Mac's Keychain";
}

export function savedMessage(os: string): string {
  return os === "windows" ? "Connected. Saved in Windows Credential Manager." : "Connected. Saved in your Mac's Keychain.";
}

export function keysFromStatus(st: KeysStatus, os: string, prev: Record<HeadId, KeyCard>): Record<HeadId, KeyCard> {
  const out = { ...prev };
  for (const p of PROVIDERS) {
    const row = st.heads[p.id];
    if (row?.has_key) {
      out[p.id] = {
        status: "connected",
        source: row.source,
        message: row.source === "keychain" ? savedMessage(os) : "Connected. Using the key from the command-line version.",
      };
    } else if (out[p.id]?.status === "connected") {
      out[p.id] = { status: "empty" };
    }
  }
  return out;
}

export function keyCardFromResult(r: KeySaveResult, os: string): KeyCard {
  if (r.ok) return { status: "connected", source: "keychain", message: savedMessage(os) };
  return { status: "error", message: r.message || "That key did not work. Check you copied all of it." };
}

/** A key looks like one long word: no spaces inside, at least 8 characters. */
export function looksLikeKey(raw: string): boolean {
  const t = raw.trim().replace(/^["']|["']$/g, "");
  return t.length >= 8 && !/\s/.test(t);
}

export function setupFailedMessage(attempts: number): string {
  return attempts >= 2
    ? "Setup did not finish again. Click Get help and we will take a look."
    : "Setup did not finish. Click Try again. If it fails twice, click Get help.";
}

// ---- Asking ----

/** The Rounds control: 1 to 4, default 2. */
export const MAX_ROUNDS = 4;
export const DEFAULT_ROUNDS = 2;
/** Kept for callers that still say "talk": two rounds. */
export const TALK_ROUNDS = DEFAULT_ROUNDS;

export type Mode = "quick" | "talk";

export function roundsOf(r: number | Mode): number {
  if (r === "quick") return 1;
  if (r === "talk") return TALK_ROUNDS;
  return Math.min(MAX_ROUNDS, Math.max(1, Math.round(r) || 1));
}

/** The most requests a run could make: one per seat per round, plus the chair each round after one. */
export function requestEstimate(seatCount: number, rounds: number | Mode): number {
  const r = roundsOf(rounds);
  if (seatCount <= 0) return 0;
  return r === 1 ? seatCount : seatCount * r + r;
}

/** The line under the ask box. `aiCount` AIs hold `seatCount` seats. */
export function estimateLine(aiCount: number, rounds: number | Mode, seatCount = aiCount): string {
  const r = roundsOf(rounds);
  const n = requestEstimate(seatCount, r);
  const ais = aiCount === 1 ? "1 AI" : `${aiCount} AIs`;
  if (aiCount === 0) return "Connect an AI in Settings first.";
  const seats = seatCount !== aiCount ? ` in ${seatCount} seats` : "";
  return r === 1 ? `This will ask ${ais}${seats}, about ${n} request${n === 1 ? "" : "s"}.` : `This will ask ${ais}${seats}, ${r} rounds, at most ${n} requests.`;
}

/** The words under each Rounds choice. */
export function roundsHint(r: number): string {
  if (r === 1) return "Each seat answers once, on its own.";
  return `They read each other and revise ${r === 2 ? "once" : `${r - 1} times`}. Claude, as chair, says where they agree.`;
}

// ---- Seats ----

/** The label every lab seat carries. */
export function labLabel(lab: string): string {
  return `Simulated perspective drawn from public positions. Not affiliated with or endorsed by ${lab}.`;
}

export function slug(s: string): string {
  let out = "";
  for (const c of s.trim()) {
    if (/[A-Za-z0-9]/.test(c)) out += c.toLowerCase();
    else if (out && !out.endsWith("-")) out += "-";
  }
  return out.replace(/-+$/, "");
}

export function allPersonas(lib: PersonaLibrary | null): Persona[] {
  return lib ? lib.packs.flatMap((p) => p.personas) : [];
}

export function allPresets(lib: PersonaLibrary | null): Preset[] {
  return lib ? lib.packs.flatMap((p) => p.presets) : [];
}

/** Find a persona by id, then name, then slug (same order as the engines). */
export function findPersona(lib: PersonaLibrary | null, key: string): Persona | undefined {
  const all = allPersonas(lib);
  const k = key.trim();
  const ks = slug(k);
  return all.find((x) => x.id === k) ?? all.find((x) => x.name.toLowerCase() === k.toLowerCase()) ?? all.find((x) => x.id === ks || slug(x.name) === ks);
}

/** The personas a Seats choice names, in order. A preset or pack whose seats are gone gives []. */
export function choicePersonas(choice: string, lib: PersonaLibrary | null): (Persona | null)[] {
  if (!choice) return [];
  const [kind, id] = [choice.slice(0, choice.indexOf(":")), choice.slice(choice.indexOf(":") + 1)];
  if (kind === "preset") {
    const pr = allPresets(lib).find((p) => p.id === id);
    return pr ? pr.seats.map((s) => findPersona(lib, s) ?? null).filter((x): x is Persona => !!x) : [];
  }
  if (kind === "pack") return lib?.packs.find((p) => p.id === id)?.personas ?? [];
  return [];
}

/** Put personas on the AIs at the table: a persona sits on its preferred AI when that AI
 * is here, every other seat on the least-used AI, in table order (same as the engines). */
export function placeSeats(personas: (Persona | null)[], heads: string[]): SeatPick[] {
  if (!heads.length) return [];
  const at: (string | null)[] = personas.map((p) => (p?.ai && heads.includes(p.ai) ? p.ai : null));
  for (let i = 0; i < at.length; i++) {
    if (at[i]) continue;
    let best = heads[0];
    let bestN = Infinity;
    for (const h of heads) {
      const n = at.filter((x) => x === h).length;
      if (n < bestN) {
        best = h;
        bestN = n;
      }
    }
    at[i] = best;
  }
  return personas.map((p, i) => ({ persona: p?.id ?? "", head: at[i]! }));
}

/** The seats a question will use. Generic is one plain seat per AI. */
export function planSeats(ask: Pick<AskBox, "seats" | "seatList">, heads: string[], lib: PersonaLibrary | null): SeatPick[] {
  if (ask.seats === "custom") {
    const ok = ask.seatList.filter((s) => heads.includes(s.head) && (!s.persona || findPersona(lib, s.persona)));
    return ok.length ? ok : heads.map((h) => ({ persona: "", head: h }));
  }
  const ps = choicePersonas(ask.seats, lib);
  return ps.length ? placeSeats(ps, heads) : heads.map((h) => ({ persona: "", head: h }));
}

/** The `--personas` value for these seats. "" when every seat is Generic, one per AI. */
export function seatSpec(seats: SeatPick[], heads: string[]): string {
  const generic = seats.every((s) => !s.persona) && seats.length === heads.length && seats.every((s, i) => s.head === heads[i]);
  return generic ? "" : seats.map((s) => `${s.persona || "generic"}@${s.head}`).join(",");
}

/** Seat keys, as the engines make them: head id, or `<persona>@<head>`, `#2` on repeats. */
export function seatKeys(seats: SeatPick[]): string[] {
  const out: string[] = [];
  for (const s of seats) {
    const base = s.persona ? `${s.persona}@${s.head}` : s.head;
    let k = base;
    for (let n = 2; out.includes(k); n++) k = `${base}#${n}`;
    out.push(k);
  }
  return out;
}

/** The SeatInfo the engine will report, worked out before the run starts. */
export function seatInfos(seats: SeatPick[], lib: PersonaLibrary | null): SeatInfo[] {
  const keys = seatKeys(seats);
  return seats.map((s, i) => {
    const p = s.persona ? findPersona(lib, s.persona) : undefined;
    return { seat: keys[i], head: s.head, persona: p?.name, expertise: p?.expertise || undefined, label: p?.lab ? labLabel(p.lab) : undefined };
  });
}

function cardFor(info: SeatInfo): HeadCard {
  return { head: info.head, seat: info.seat, persona: info.persona, expertise: info.expertise, label: info.label, status: "waiting", text: "", round: 1 };
}

/** A new run. `seats` defaults to one Generic seat per head. */
export function newRun(question: string, heads: string[], rounds: number | Mode, research = false, context: string | null = null, seats?: SeatInfo[]): RunView {
  const infos = seats ?? heads.map((h) => ({ seat: h, head: h }));
  const r = roundsOf(rounds);
  const cards: Record<string, HeadCard> = {};
  const byKey: Record<string, SeatInfo> = {};
  for (const i of infos) {
    cards[i.seat] = cardFor(i);
    byKey[i.seat] = i;
  }
  return {
    question,
    mode: r > 1 ? "talk" : "quick",
    research,
    context,
    heads: infos.map((i) => i.seat),
    seats: byKey,
    steps: [],
    cards,
    verdicts: [],
    round: 1,
    rounds: r,
    done: false,
    stopped: false,
    transcript: "",
    answered: 0,
  };
}

const STATUS: Record<string, CardStatus> = {
  answered: "answered",
  skipped: "skipped",
  timed_out: "timed_out",
  failed: "failed",
};

function stepFor(r: RunView, round: number): RoundStep {
  let st = r.steps.find((x) => x.round === round);
  if (!st) {
    st = { round, answers: {} };
    r.steps = [...r.steps, st].sort((a, b) => a.round - b.round);
  }
  return st;
}

/** Fold one engine event into the run. Returns a new object. */
export function applyEvent(run: RunView, e: EngineEvent): RunView {
  const r: RunView = { ...run, cards: { ...run.cards }, verdicts: [...run.verdicts], steps: run.steps.map((x) => ({ ...x, answers: { ...x.answers } })) };
  switch (e.event) {
    case "run_started":
      r.rounds = e.rounds;
      r.mode = e.rounds > 1 ? "talk" : "quick";
      if (e.seats?.length) {
        r.heads = e.seats.map((s) => s.seat);
        r.seats = Object.fromEntries(e.seats.map((s) => [s.seat, s]));
        for (const s of e.seats) r.cards[s.seat] = { ...cardFor(s), ...(r.cards[s.seat] ?? {}), persona: s.persona, expertise: s.expertise, label: s.label, head: s.head, seat: s.seat };
      } else {
        r.heads = e.heads;
        for (const h of e.heads) if (!r.cards[h]) r.cards[h] = { head: h, seat: h, status: "waiting", text: "", round: 1 };
      }
      break;
    case "head_started": {
      const k = e.seat ?? e.head;
      const prev = r.cards[k];
      r.round = e.round;
      // A later round keeps the last answer on screen while the AI revises it.
      r.cards[k] = { ...(prev ?? { head: e.head, seat: k }), head: e.head, status: "thinking", text: prev?.text ?? "", round: e.round, message: undefined, fix: undefined };
      break;
    }
    case "head_done": {
      const k = e.seat ?? e.head;
      const prev = r.cards[k];
      const status = STATUS[e.status] ?? "failed";
      r.cards[k] = {
        ...(prev ?? { head: e.head, seat: k }),
        head: e.head,
        status,
        text: status === "answered" ? e.text : prev?.text ?? "",
        message: status === "answered" ? undefined : e.message,
        fix: status === "answered" ? undefined : e.fix,
        via: e.via,
        round: e.round,
      };
      stepFor(r, e.round).answers[k] = { status, text: status === "answered" ? e.text : "" };
      if (status === "answered" && run.done) r.answered = Object.values(r.cards).filter((c) => c.status === "answered").length;
      break;
    }
    case "retry_done": {
      const i = r.heads.indexOf(e.seat ?? e.head);
      if (i >= 0 && r.transcript) r.transcript = replaceSection(r.transcript, i, firstSection(e.transcript));
      break;
    }
    case "chair_verdict": {
      const v = { round: e.round, consensus: e.consensus, text: e.text };
      r.verdicts.push(v);
      stepFor(r, e.round).verdict = v;
      break;
    }
    case "run_done":
      r.done = true;
      r.transcript = e.transcript;
      r.answered = e.answered;
      r.consensus = e.consensus;
      break;
    case "process_exit":
      r.done = true;
      break;
    case "round_done":
      break;
  }
  return r;
}

/** The answer text with the engine's "POSITION:" lead removed. */
export function cleanAnswer(t: string): string {
  return t.replace(/^\s*\**POSITION:\**\s*/i, "");
}

/** The `## ` sections of an advisory transcript, by position: [start, end) offsets. */
function sectionSpans(t: string): [number, number][] {
  const spans: [number, number][] = [];
  const re = /^## .*$/gm;
  let m: RegExpExecArray | null;
  const starts: number[] = [];
  while ((m = re.exec(t))) starts.push(m.index);
  for (let i = 0; i < starts.length; i++) {
    const nextStart = i + 1 < starts.length ? starts[i + 1] : t.length;
    // A section ends at the `---` line before the next heading or the footer.
    const sep = t.lastIndexOf("\n---", nextStart);
    spans.push([starts[i], sep > starts[i] ? sep + 1 : nextStart]);
  }
  return spans;
}

/** The first `## ` section of a transcript, heading and body. */
export function firstSection(t: string): string {
  const s = sectionSpans(t)[0];
  return s ? t.slice(s[0], s[1]) : "";
}

/** Swap the section at `index` for `section`. Unchanged when either is missing. */
export function replaceSection(t: string, index: number, section: string): string {
  const s = sectionSpans(t)[index];
  if (!s || !section) return t;
  return t.slice(0, s[0]) + section + t.slice(s[1]);
}

/** A failed head can be asked again on its own in Quick answers, once the run is done. */
export function canRetry(run: RunView, c: HeadCard): boolean {
  return run.mode === "quick" && run.done && !run.stopped && (c.status === "failed" || c.status === "timed_out" || c.status === "skipped");
}

/** Every AI failed or was skipped. */
export function allFailed(run: RunView): boolean {
  return run.done && !Object.values(run.cards).some((c) => c.status === "answered" || c.text !== "");
}

/** The first failure's plain message, for "Back to keys". */
export function firstFailure(run: RunView): string {
  const c = Object.values(run.cards).find((c) => c.message);
  return c?.message ?? "None of the AIs answered. Check your keys and try again.";
}

export function cardStatusText(c: HeadCard): string {
  switch (c.status) {
    case "waiting":
      return "Waiting";
    case "thinking":
      return c.round > 1 ? `Reading the others (round ${c.round})` : "Asking...";
    case "answered":
      return c.round > 1 ? `Answered (round ${c.round})` : "Answered";
    case "skipped":
      return "Not set up";
    case "timed_out":
      return "Took too long";
    case "failed":
      return "Failed";
  }
}

/** Split the chair's text into the verdict line and the rest. */
export function splitVerdict(text: string): { agree: boolean; body: string } {
  const lines = text.split("\n");
  const first = (lines[0] || "").toLowerCase();
  const agree = /verdict:\s*\**\s*consensus/.test(first);
  const body = /verdict:/.test(first) ? lines.slice(1).join("\n").trim() : text.trim();
  return { agree, body };
}

// ---- History ----

export function filterHistory(list: HistoryEntry[], q: string): HistoryEntry[] {
  const t = q.trim().toLowerCase();
  if (!t) return list;
  return list.filter((e) => e.question.toLowerCase().includes(t));
}

export function firstLine(s: string, max = 70): string {
  const l = (s.split("\n")[0] || "").trim();
  return l.length > max ? l.slice(0, max - 1) + "…" : l;
}

export function shortDate(ts: number, now = Date.now()): string {
  const d = new Date(ts * 1000);
  const today = new Date(now);
  if (d.toDateString() === today.toDateString()) {
    return d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  }
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Status dot: green when the engine runs and at least one AI is ready. */
export function statusDot(s: Pick<State, "engineReady" | "keys"> & Partial<Pick<State, "clis" | "settings">>): "ok" | "warn" {
  return s.engineReady && connectedHeads(s.keys, cliHeadsOf(s)).length > 0 ? "ok" : "warn";
}

/** Who sits at the table: the saved choice, limited to connected AIs; else every connected AI. */
export function tableHeads(settings: Settings, keys: Record<string, KeyCard>, cliHeads: string[] = []): string[] {
  const connected = connectedHeads(keys, cliHeads);
  const chosen = settings.default_heads.filter((h) => connected.includes(h));
  return chosen.length ? chosen : connected;
}

export function helpUrl(base: string, code?: string, os?: string, version?: string): string {
  const title = code ? `Setup problem ${code}` : "Problem report";
  const body = `What happened:\n\n\nApp version: ${version ?? ""}\nSystem: ${os ?? ""}${code ? `\nError code: ${code}` : ""}`;
  return `${base}?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`;
}

export type { ClaudeStatus };
