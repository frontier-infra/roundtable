// The app's backend, as the UI sees it. `tauriApi` talks to the Rust core;
// `mock.ts` gives the same shape for tests and the browser preview.

import type { DecisionEvent, DecisionSettings } from "./decision";

export type HeadId = "claude" | "gemini" | "openai" | "grok" | "glm" | "minimax";

export interface AppInfo {
  version: string;
  os: string;
  arch: string;
  /** Debug builds only: the screen to open on ("3" or "settings"). */
  start?: string;
}

export interface EngineStatus {
  installed: boolean;
  current: boolean;
  version: string | null;
  expected: string;
}

export interface SetupFailure {
  code: string;
  step: number;
  detail: string;
}

export interface KeyStatusRow {
  provider: string;
  has_key: boolean;
  source: "env" | "keychain" | "file" | null;
}

export interface KeysStatus {
  heads: Record<string, KeyStatusRow>;
  importable: string[];
}

export interface KeySaveResult {
  ok: boolean;
  reason?: string;
  message?: string;
}

export interface ClaudeStatus {
  app_found: boolean;
  app_connected: boolean;
  code_found: boolean;
  code_connected: boolean;
  plugin_ids: string[];
  plugin_lines: [string, string];
}

/** An AI tool found on this computer (Claude Code, Codex, Gemini CLI, ...). */
export interface CliDetection {
  id: string;
  /** The head it seats at the table. */
  head: string;
  name: string;
  found: boolean;
  path?: string | null;
  version: string | null;
  /** null when the tool gives no way to tell without sending a prompt. */
  signed_in: boolean | null;
  /** One sentence: whose account pays. */
  note: string;
  /** How Roundtable calls it. */
  how: string;
  /** The tool's own sign-in command. */
  login: string;
}

export interface ActionResult {
  ok: boolean;
  message?: string;
}

export interface HistoryEntry {
  id: string;
  ts: number;
  question: string;
  source: string;
  mode: string;
  heads: string[];
  consensus?: boolean;
}

export interface Settings {
  default_heads: string[];
  models: Record<string, string>;
  timeout_secs: number | null;
  history: boolean;
  auto_update: boolean;
  onboarded: boolean;
  terminal_command: boolean;
  /** Head ids that run through the person's own installed tool. */
  cli_heads: string[];
  /** The person ticked or unticked a tool, so detection stops choosing for them. */
  clis_chosen: boolean;
  /** Show the Frontier labs persona pack. */
  show_lab_pack: boolean;
  /** The ask box's last Seats choice: "", "preset:<id>" or "pack:<id>". */
  personas: string;
  /** The question coach opens before a question goes to the table. */
  coach: boolean;
  /** Round 1 is blind: seats answer without seeing each other. */
  blind_first_round: boolean;
  /** The verdict shape the ask box starts with. */
  verdict_shape: VerdictShape;
  /** The decision model that rates the verdict. Off when missing or provider is null. */
  decision?: DecisionSettings;
  /** End Markdown exports with the one Roundtable credit line. */
  export_credit: boolean;
  /** Providers added in Settings (owned by `roundtable provider`; the app reads them only). */
  providers?: ProviderConfig[];
}

// ---- Providers the person adds: OpenRouter, Ollama, LM Studio, Groq, Together, Custom ----

/** One ticked model: a seat. Prices are US dollars per million tokens, when published. */
export interface SeatModel {
  id: string;
  price_in?: number | null;
  price_out?: number | null;
}

/** An added provider as settings.json keeps it. Never a key. */
export interface ProviderConfig {
  id: string;
  preset: string;
  name: string;
  base_url: string;
  needs_key: boolean;
  models: SeatModel[];
}

export interface ProviderPreset {
  id: string;
  name: string;
  base_url: string;
  needs_key: boolean;
  local: boolean;
  key_page: string;
}

/** `roundtable provider list --json`: one added provider. */
export interface ProviderRow {
  id: string;
  preset: string;
  name: string;
  base_url: string;
  needs_key: boolean;
  has_key: boolean;
  key_source: "env" | "keychain" | null;
  local: boolean;
  note: string | null;
  key_page: string;
  /** Local, or has its key. */
  ready: boolean;
  models: (SeatModel & { seat: string })[];
}

export interface ProvidersList {
  providers: ProviderRow[];
  presets: ProviderPreset[];
}

/** One model a provider lists. */
export interface ModelInfo {
  id: string;
  name?: string;
  context?: number;
  price_in?: number;
  price_out?: number;
  /** Runs on this computer. */
  free: boolean;
  /** One of our frontier-infra Jebadiah builds. Any other model named Jebadiah is not. */
  ours: boolean;
  note?: string;
}

export interface ModelsResult {
  ok: boolean;
  provider?: string;
  local?: boolean;
  total?: number;
  models?: ModelInfo[];
  ticked?: string[];
  reason?: string;
  message?: string;
}

/** Ollama or LM Studio, found running on this computer. */
export interface LocalProvider {
  preset: string;
  name: string;
  base_url: string;
  models: ModelInfo[];
  /** "free, on this computer". */
  note: string;
}

export interface ProviderAddRequest {
  preset: string;
  id?: string;
  name?: string;
  url?: string;
  /** Goes straight to the engine's stdin, then the keychain. */
  key?: string;
  /** The ticked models. Each one is a seat. Leave out to keep the ticks. */
  models?: string[];
  /** Check the address and key with one model list before saving. */
  check?: boolean;
}

export interface ProviderAddResult {
  ok: boolean;
  reason?: string;
  message?: string;
  seats?: string[];
  provider?: ProviderRow;
}

/** The chair's final verdict: a decision with confidence, pros and cons, an action plan, or a vote. */
export type VerdictShape = "decision" | "pros-cons" | "plan" | "vote";

/** The coach's brief. The table reads it in front of the question. */
export interface Brief {
  goal: string;
  context: string;
  constraints: string;
  /** What a good answer looks like. */
  good_answer: string;
}

export interface CoachQuestions {
  ok: boolean;
  questions: string[];
  message?: string;
}

export interface CoachBrief {
  ok: boolean;
  brief: Brief;
  markdown: string;
  message?: string;
}

/** Seats the app suggests for a question or brief. */
export interface PanelSuggestion {
  /** "chair", "words" or "none". */
  source: string;
  personas: string[];
  why: string;
  seats: { seat: string; head: string; persona: string | null; name: string | null }[];
  spec: string;
}

/** A file or link, read once. Every seat reads `text`. */
export interface Attachment {
  name: string;
  /** "text", "pdf" or "link". */
  kind: string;
  chars: number;
  /** "full", "summary" or "start". */
  shared: string;
  text: string;
}

export interface AttachResult {
  ok: boolean;
  attachments?: Attachment[];
  block?: string;
  message?: string;
}

/** What the Decision model setting needs to say how to get Jebadiah. */
export interface JebadiahLocal {
  os: string;
  arch: string;
  /** Total memory in GB, or null when unknown. */
  memory_gb: number | null;
  /** Our build, found in Ollama or LM Studio on this computer. */
  found: { app: string; model: string } | null;
  /** What `roundtable decision detect` found: a Jebadiah server (jeb serve, AINode) Roundtable can ask. */
  served: { url: string; model: string; via: string } | null;
  /** The engine's next step when no server answered. */
  hint?: string | null;
  looked: string[];
}

/** A seat role: it sits on an AI seat and shapes how that seat reasons. */
export interface Persona {
  id: string;
  name: string;
  expertise: string;
  brief: string;
  /** Preferred head id. */
  ai?: string;
  /** On lab seats: the lab whose public view this simulates. */
  lab?: string;
}

export interface Preset {
  id: string;
  name: string;
  description: string;
  seats: string[];
}

export interface Pack {
  id: string;
  name: string;
  description: string;
  labs?: boolean;
  personas: Persona[];
  presets: Preset[];
  builtin: boolean;
}

export interface PersonaLibrary {
  packs: Pack[];
  lab_pack_built: boolean;
  labs_hidden: boolean;
  custom_pack: string;
}

/** One seat at the start of a run. */
export interface SeatInfo {
  seat: string;
  head: string;
  persona?: string;
  expertise?: string;
  /** The lab label on a lab seat. */
  label?: string;
}

export interface AskRequest {
  question: string;
  heads: string[];
  rounds: number;
  research: boolean;
  contextPath?: string | null;
  contextText?: string | null;
  /** Seat spec for `--personas`, e.g. "cfo@claude,ceo@gemini". Empty is Generic. */
  personas?: string;
  /** The coach's brief, as Markdown. */
  brief?: string | null;
  verdict?: VerdictShape | null;
  /** Round 1 open: each seat reads the seats before it. */
  openFirstRound?: boolean;
  /** Earlier turns of this session, for a follow-up. */
  prior?: string | null;
  /** Files and links, already read once (`attachPrepare`). */
  attached?: string | null;
  /** "setup": the first-run sample, kept out of history. */
  source?: "setup" | null;
}

/** One line from `roundtable ask --events`, plus `process_exit` from the app. */
export type EngineEvent =
  | { event: "run_started"; heads: string[]; rounds: number; mode: string; seats?: SeatInfo[] }
  | { event: "head_started"; head: string; round: number; seat?: string }
  | { event: "head_done"; head: string; round: number; status: string; text: string; reason?: string; message?: string; fix?: Fix; via?: string; seat?: string }
  | { event: "round_done"; round: number }
  | { event: "chair_verdict"; round: number; consensus: boolean; text: string }
  | {
      event: "verdict";
      shape: VerdictShape;
      text: string;
      minority?: string;
      held_by?: string;
      confidence?: string;
      decision?: string;
      would_change?: string;
      next_steps?: string[];
      questions?: string[];
    }
  /** The question lacks key facts the pick depends on: the chair opens with these. */
  | { event: "clarify"; questions: string[] }
  | { event: "run_done"; transcript: string; answered: number; consensus?: boolean; history_id?: string }
  | { event: "process_exit" }
  /** A one-head retry finished. Its transcript holds just that head. */
  | { event: "retry_done"; head: string; transcript: string; seat?: string }
  /** The decision model rated the verdict (only when one is on). */
  | DecisionEvent;

/** What to do about a head that did not answer. */
export type Fix = "sign_in" | "add_key" | "install" | "retry";

/** `roundtable test <ai> --json`: one five-word question, and how it went. */
export interface HeadTest {
  head: string;
  name: string;
  ok: boolean;
  status: string;
  /** The tool's name, or "API key". */
  via: string | null;
  first_line: string | null;
  message: string | null;
  fix: Fix | null;
  secs: number;
  command: string | null;
  exit: number | null;
}

export interface RetryRequest {
  question: string;
  research: boolean;
  /** A file path, or "follow-up" for the last roundtable's transcript. */
  contextPath?: string | null;
  /** The seat key, and its persona id on a persona seat. */
  seat?: string;
  persona?: string;
  brief?: string | null;
  prior?: string | null;
  attached?: string | null;
}

export interface UpdateInfo {
  version: string;
  install: () => Promise<void>;
}

export interface Api {
  appInfo(): Promise<AppInfo>;
  engineStatus(): Promise<EngineStatus>;
  engineSetup(onStep: (i: number) => void): Promise<void>;
  keysStatus(): Promise<KeysStatus>;
  keySave(head: string, key: string): Promise<KeySaveResult>;
  keyRemove(head: string): Promise<void>;
  keysImport(remove: boolean): Promise<{ ok: boolean; imported?: string[] }>;
  clisDetect(): Promise<CliDetection[]>;
  providersList(): Promise<ProvidersList>;
  /** Ollama and LM Studio running on this computer. Sends no prompt. */
  providersDetect(): Promise<{ found: LocalProvider[] }>;
  providerModels(id: string): Promise<ModelsResult>;
  providerAdd(req: ProviderAddRequest): Promise<ProviderAddResult>;
  providerRemove(id: string): Promise<ActionResult>;
  cliLogin(id: string): Promise<ActionResult>;
  claudeStatus(): Promise<ClaudeStatus>;
  claudeConnectApp(): Promise<ActionResult>;
  claudeConnectCode(): Promise<ActionResult>;
  claudeDisablePlugin(): Promise<ActionResult>;
  claudeDisconnect(): Promise<ActionResult>;
  askStart(req: AskRequest, onEvent: (e: EngineEvent) => void): Promise<void>;
  askStop(): Promise<void>;
  /** Ask one head of the current run again. Its events arrive through the run's listener. */
  askRetry(head: string, req: RetryRequest): Promise<void>;
  /** Ask one AI a five-word question. `via`: "auto", "app" or "key". */
  headTest(head: string, via: "auto" | "app" | "key"): Promise<HeadTest>;
  /** The coach's questions (at most three). */
  coachQuestions(question: string): Promise<CoachQuestions>;
  coachBrief(question: string, qa: { q: string; a: string }[]): Promise<CoachBrief>;
  suggestPanel(question: string, brief: string | null, heads: string[]): Promise<PanelSuggestion>;
  /** Read a file or link once, as every seat will read it. */
  attachPrepare(source: string): Promise<AttachResult>;
  /** Pick a text file or PDF. */
  pickAttachment(): Promise<string | null>;
  personasList(): Promise<PersonaLibrary>;
  /** Create (empty id) or update one of the person's own personas. */
  personaSave(p: Persona): Promise<Persona>;
  personaDuplicate(id: string): Promise<Persona>;
  personaDelete(id: string): Promise<void>;
  /** Asks where to save, then writes the person's own pack. null when cancelled. */
  personasExport(): Promise<number | null>;
  /** Asks for a pack file, then adds its personas. null when cancelled. */
  personasImport(): Promise<number | null>;
  historyList(): Promise<HistoryEntry[]>;
  historyGet(id: string): Promise<string | null>;
  /** Take roundtables out of history. Returns a token for Undo (historyRestore) or historyPurge. */
  historyDelete(ids: string[]): Promise<string>;
  /** Undo a delete. Returns how many came back. */
  historyRestore(token: string): Promise<number>;
  /** The Undo window closed: delete the files for good. */
  historyPurge(token: string): Promise<void>;
  historyClear(): Promise<void>;
  settingsGet(): Promise<Settings>;
  settingsSet(s: Settings): Promise<void>;
  terminalCommand(on: boolean): Promise<string>;
  diagnostics(): Promise<string>;
  openUrl(url: string): Promise<void>;
  /** Memory, chip, and whether our decision model is already in Ollama or LM Studio. */
  jebadiahLocal(): Promise<JebadiahLocal>;
  pickTextFile(): Promise<string | null>;
  saveTextFile(defaultName: string, text: string): Promise<boolean>;
  copyText(text: string): Promise<void>;
  checkUpdate(): Promise<UpdateInfo | null>;
  restart(): Promise<void>;
}

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return "Something went wrong.";
}

export async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const opener = await import("@tauri-apps/plugin-opener");
  const dialog = await import("@tauri-apps/plugin-dialog");

  let currentRun = 0;
  let unlistenRun: (() => void) | null = null;

  return {
    appInfo: () => invoke("app_info"),
    engineStatus: () => invoke("engine_status"),
    async engineSetup(onStep) {
      const un = await listen<number>("setup-step", (e) => onStep(e.payload));
      try {
        await invoke("engine_setup");
      } finally {
        un();
      }
    },
    keysStatus: () => invoke("keys_status"),
    keySave: (head, key) => invoke("key_save", { head, key }),
    keyRemove: async (head) => {
      await invoke("key_remove", { head });
    },
    keysImport: (remove) => invoke("keys_import", { remove }),
    clisDetect: () => invoke("clis_detect"),
    providersList: () => invoke("providers_list"),
    providersDetect: () => invoke("providers_detect"),
    providerModels: (id) => invoke("provider_models", { id }),
    providerAdd: (req) =>
      invoke("provider_add", { preset: req.preset, id: req.id ?? null, name: req.name ?? null, url: req.url ?? null, key: req.key ?? null, models: req.models ?? null, check: !!req.check }),
    providerRemove: (id) => invoke("provider_remove", { id }),
    cliLogin: (id) => invoke("cli_login", { id }),
    claudeStatus: () => invoke("claude_status"),
    claudeConnectApp: () => invoke("claude_connect_app"),
    claudeConnectCode: () => invoke("claude_connect_code"),
    claudeDisablePlugin: () => invoke("claude_disable_plugin"),
    claudeDisconnect: () => invoke("claude_disconnect"),
    async askStart(req, onEvent) {
      unlistenRun?.();
      unlistenRun = await listen<{ run: number; data: EngineEvent }>("rt-event", (e) => {
        if (e.payload.run === currentRun) onEvent(e.payload.data);
      });
      currentRun = await invoke<number>("ask_start", {
        question: req.question,
        heads: req.heads,
        rounds: req.rounds,
        research: req.research,
        contextPath: req.contextPath ?? null,
        contextText: req.contextText ?? null,
        personas: req.personas ?? null,
        brief: req.brief ?? null,
        verdict: req.verdict ?? null,
        openFirstRound: req.openFirstRound ?? null,
        prior: req.prior ?? null,
        attached: req.attached ?? null,
        source: req.source ?? null,
      });
    },
    askStop: () => invoke("ask_stop"),
    askRetry: (head, req) =>
      invoke("ask_retry", { run: currentRun, head, question: req.question, research: req.research, contextPath: req.contextPath ?? null, seat: req.seat ?? null, persona: req.persona ?? null, brief: req.brief ?? null, prior: req.prior ?? null, attached: req.attached ?? null }),
    headTest: (head, via) => invoke("head_test", { head, via }),
    coachQuestions: (question) => invoke("coach_questions", { question }),
    coachBrief: (question, qa) => invoke("coach_brief", { question, qa }),
    suggestPanel: (question, brief, heads) => invoke("suggest_panel", { question, brief, heads }),
    attachPrepare: (source) => invoke("attach_prepare", { source }),
    async pickAttachment() {
      const p = await dialog.open({
        multiple: false,
        directory: false,
        filters: [{ name: "Text or PDF", extensions: ["txt", "md", "markdown", "csv", "json", "pdf", "html", "htm"] }],
      });
      return typeof p === "string" ? p : null;
    },
    personasList: () => invoke("personas_list"),
    personaSave: (persona) => invoke("persona_save", { persona }),
    personaDuplicate: (id) => invoke("persona_duplicate", { id }),
    personaDelete: async (id) => {
      await invoke("persona_delete", { id });
    },
    async personasExport() {
      const p = await dialog.save({ defaultPath: "my-personas.json", filters: [{ name: "Persona pack", extensions: ["json"] }] });
      if (!p) return null;
      return invoke<number>("personas_export", { path: p });
    },
    async personasImport() {
      const p = await dialog.open({ multiple: false, directory: false, filters: [{ name: "Persona pack", extensions: ["json"] }] });
      if (typeof p !== "string") return null;
      return invoke<number>("personas_import", { path: p });
    },
    historyList: () => invoke("history_list"),
    historyGet: (id) => invoke("history_get", { id }),
    historyDelete: (ids) => invoke("history_delete", { ids }),
    historyRestore: (token) => invoke("history_restore", { token }),
    historyPurge: (token) => invoke("history_purge", { token }),
    historyClear: () => invoke("history_clear"),
    settingsGet: () => invoke("settings_get"),
    settingsSet: (settings) => invoke("settings_set", { settings }),
    terminalCommand: (on) => invoke("terminal_command", { on }),
    diagnostics: () => invoke("diagnostics"),
    openUrl: (url) => opener.openUrl(url),
    jebadiahLocal: () => invoke("jebadiah_local"),
    async pickTextFile() {
      const p = await dialog.open({
        multiple: false,
        directory: false,
        filters: [{ name: "Text", extensions: ["txt", "md", "markdown", "csv", "json"] }],
      });
      return typeof p === "string" ? p : null;
    },
    async saveTextFile(defaultName, text) {
      const p = await dialog.save({ defaultPath: defaultName, filters: [{ name: "Markdown", extensions: ["md"] }] });
      if (!p) return false;
      await invoke("save_text", { path: p, text });
      return true;
    },
    copyText: (text) => navigator.clipboard.writeText(text),
    async checkUpdate() {
      const { check } = await import("@tauri-apps/plugin-updater");
      const u = await check();
      if (!u) return null;
      return { version: u.version, install: () => u.downloadAndInstall() };
    },
    async restart() {
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
  };
}
