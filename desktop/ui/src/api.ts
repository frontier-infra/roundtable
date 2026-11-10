// The app's backend, as the UI sees it. `tauriApi` talks to the Rust core;
// `mock.ts` gives the same shape for tests and the browser preview.

export type HeadId = "claude" | "gemini" | "openai" | "grok" | "glm" | "minimax";

export interface AppInfo {
  version: string;
  os: string;
  arch: string;
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
}

export interface AskRequest {
  question: string;
  heads: string[];
  rounds: number;
  research: boolean;
  contextPath?: string | null;
  contextText?: string | null;
}

/** One line from `roundtable ask --events`, plus `process_exit` from the app. */
export type EngineEvent =
  | { event: "run_started"; heads: string[]; rounds: number; mode: string }
  | { event: "head_started"; head: string; round: number }
  | { event: "head_done"; head: string; round: number; status: string; text: string; reason?: string; message?: string }
  | { event: "round_done"; round: number }
  | { event: "chair_verdict"; round: number; consensus: boolean; text: string }
  | { event: "run_done"; transcript: string; answered: number; consensus?: boolean; history_id?: string }
  | { event: "process_exit" };

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
  claudeStatus(): Promise<ClaudeStatus>;
  claudeConnectApp(): Promise<ActionResult>;
  claudeConnectCode(): Promise<ActionResult>;
  claudeDisablePlugin(): Promise<ActionResult>;
  claudeDisconnect(): Promise<ActionResult>;
  askStart(req: AskRequest, onEvent: (e: EngineEvent) => void): Promise<void>;
  askStop(): Promise<void>;
  historyList(): Promise<HistoryEntry[]>;
  historyGet(id: string): Promise<string | null>;
  historyDelete(id: string): Promise<void>;
  historyClear(): Promise<void>;
  settingsGet(): Promise<Settings>;
  settingsSet(s: Settings): Promise<void>;
  terminalCommand(on: boolean): Promise<string>;
  diagnostics(): Promise<string>;
  openUrl(url: string): Promise<void>;
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
      });
    },
    askStop: () => invoke("ask_stop"),
    historyList: () => invoke("history_list"),
    historyGet: (id) => invoke("history_get", { id }),
    historyDelete: (id) => invoke("history_delete", { id }),
    historyClear: () => invoke("history_clear"),
    settingsGet: () => invoke("settings_get"),
    settingsSet: (settings) => invoke("settings_set", { settings }),
    terminalCommand: (on) => invoke("terminal_command", { on }),
    diagnostics: () => invoke("diagnostics"),
    openUrl: (url) => opener.openUrl(url),
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
