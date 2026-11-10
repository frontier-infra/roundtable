// Pure state for the walkthrough and the live roundtable. No DOM here, so it is easy to test.

import type { ClaudeStatus, CliDetection, EngineEvent, HeadId, HistoryEntry, KeySaveResult, KeysStatus, Settings } from "./api";
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
  message?: string;
  round: number;
}

export interface Verdict {
  round: number;
  consensus: boolean;
  text: string;
}

export interface RunView {
  question: string;
  mode: "quick" | "talk";
  heads: string[];
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
  ask: { question: string; mode: "quick" | "talk"; research: boolean; contextPath: string | null; contextText: string | null; contextName: string | null };
  history: HistoryEntry[];
  historyFilter: string;
  opened: { id: string; transcript: string } | null;
  settings: Settings;
  update: { version: string } | null;
  toast: string | null;
  engineReady: boolean;
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
    ask: { question: "", mode: "quick", research: false, contextPath: null, contextText: null, contextName: null },
    history: [],
    historyFilter: "",
    opened: null,
    settings: defaultSettings(),
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

/** Heads running through a ticked tool that is still on this computer.
 * Before detection finishes, the saved list stands. */
export function cliHeadsOn(s: Pick<State, "clis" | "settings">): string[] {
  const on = s.settings.cli_heads ?? [];
  if (!s.clis.loaded) return HEAD_ORDER.filter((h) => on.includes(h));
  const found = new Set(s.clis.list.filter((d) => d.found && d.signed_in !== false).map((d) => d.head));
  return HEAD_ORDER.filter((h) => on.includes(h) && found.has(h));
}

/** The line under a tool's name. Two sentences at most. */
export function cliCardText(d: CliDetection, os: string): string {
  if (d.signed_in === true) return `${d.name}, signed in. ${d.note}`;
  if (d.signed_in === false) return `${d.name} is installed but not signed in. Click Sign in to open it in ${os === "windows" ? "a terminal window" : "Terminal"}.`;
  return `${d.name} is installed. We could not tell if you are signed in.`;
}

type ContinueState = Pick<State, "step" | "setup" | "keys"> & Partial<Pick<State, "clis" | "settings">>;

function cliHeadsOf(s: Partial<Pick<State, "clis" | "settings">>): string[] {
  return s.clis && s.settings ? cliHeadsOn({ clis: s.clis, settings: s.settings }) : [];
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

/** Talk it through runs up to this many rounds. */
export const TALK_ROUNDS = 3;

/** The most requests a run could make: one per AI per round, plus the chair each round. */
export function requestEstimate(aiCount: number, mode: "quick" | "talk"): number {
  if (aiCount <= 0) return 0;
  return mode === "quick" ? aiCount : aiCount * TALK_ROUNDS + TALK_ROUNDS;
}

export function estimateLine(aiCount: number, mode: "quick" | "talk"): string {
  const n = requestEstimate(aiCount, mode);
  const ais = aiCount === 1 ? "1 AI" : `${aiCount} AIs`;
  if (aiCount === 0) return "Connect an AI in Settings first.";
  return mode === "quick"
    ? `This will ask ${ais}, about ${n} request${n === 1 ? "" : "s"}.`
    : `This will ask ${ais}, at most ${n} requests.`;
}

export function newRun(question: string, heads: string[], mode: "quick" | "talk"): RunView {
  const cards: Record<string, HeadCard> = {};
  for (const h of heads) cards[h] = { head: h, status: "waiting", text: "", round: 1 };
  return {
    question,
    mode,
    heads,
    cards,
    verdicts: [],
    round: 1,
    rounds: mode === "quick" ? 1 : TALK_ROUNDS,
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

/** Fold one engine event into the run. Returns a new object. */
export function applyEvent(run: RunView, e: EngineEvent): RunView {
  const r: RunView = { ...run, cards: { ...run.cards }, verdicts: [...run.verdicts] };
  switch (e.event) {
    case "run_started":
      r.heads = e.heads;
      r.rounds = e.rounds;
      for (const h of e.heads) if (!r.cards[h]) r.cards[h] = { head: h, status: "waiting", text: "", round: 1 };
      break;
    case "head_started": {
      const prev = r.cards[e.head];
      r.round = e.round;
      // A later round keeps the last answer on screen while the AI revises it.
      r.cards[e.head] = { head: e.head, status: "thinking", text: prev?.text ?? "", round: e.round };
      break;
    }
    case "head_done": {
      const prev = r.cards[e.head];
      const status = STATUS[e.status] ?? "failed";
      r.cards[e.head] = {
        head: e.head,
        status,
        text: status === "answered" ? e.text : prev?.text ?? "",
        message: e.message,
        round: e.round,
      };
      break;
    }
    case "chair_verdict":
      r.verdicts.push({ round: e.round, consensus: e.consensus, text: e.text });
      break;
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
      return c.round > 1 ? `Reading the others (round ${c.round})` : "Thinking...";
    case "answered":
      return c.round > 1 ? `Answered (round ${c.round})` : "Answered";
    case "skipped":
      return "Skipped, no key";
    case "timed_out":
      return "Took too long";
    case "failed":
      return "No answer";
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
