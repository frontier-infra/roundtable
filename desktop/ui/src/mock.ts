// A fake backend with the same shape as the Rust core. Used by the UI tests and by the
// browser preview (`?mock=1`), so every screen can be seen without keys or network.

import type { Api, ClaudeStatus, CliDetection, EngineEvent, Fix, HeadTest, HistoryEntry, KeysStatus, Settings } from "./api";
import { defaultSettings } from "./state";
import { PROVIDERS } from "./providers";

export interface MockOptions {
  os?: string;
  /** "none", "app", "code", "both" or "plugin" (both, with the old plugin installed). */
  claude?: string;
  /** Head ids that already have a key. */
  keys?: string[];
  setupFails?: boolean;
  onboarded?: boolean;
  history?: HistoryEntry[];
  /** Milliseconds between fake events. 0 runs them on the next tick. */
  delay?: number;
  /** AI tools on the computer: "none" (default), "some", or a list. */
  clis?: "none" | "some" | CliDetection[];
  /** Heads whose five-word test fails, with the engine's reason. */
  testFails?: Record<string, { message: string; fix: Fix }>;
  /** Heads that fail in a run with this reason (instead of the default bad key). */
  runFails?: Record<string, { message: string; fix: Fix }>;
}

function tool(id: string, head: string, name: string, version: string | null, signed_in: boolean | null, note: string, login: string): CliDetection {
  return { id, head, name, found: version !== null, version, signed_in: version === null ? null : signed_in, note, how: `${id} -p`, login };
}

/** A computer with Claude Code, Codex and Gemini signed in, Grok and Cursor signed out. */
export function sampleClis(): CliDetection[] {
  return [
    tool("claude", "claude", "Claude Code", "2.1.321", true, "Uses your Claude plan.", "claude auth login"),
    tool("codex", "codex", "Codex", "0.167.0", true, "Uses your ChatGPT plan.", "codex login"),
    tool("gemini", "gemini", "Gemini CLI", "0.67.0", true, "Uses your Google account.", "gemini"),
    tool("grok", "grok", "Grok", "1.0.77", false, "Uses your Grok plan.", "grok login"),
    tool("qwen", "qwen", "Qwen Code", null, null, "Uses your Qwen account.", "qwen"),
    tool("kimi", "kimi", "Kimi", null, null, "Uses your Kimi account.", "kimi login"),
    tool("opencode", "opencode", "OpenCode", null, null, "Uses the accounts you set up in OpenCode.", "opencode auth login"),
    tool("cursor", "cursor", "Cursor Agent", "2026.11.05", false, "Uses your Cursor plan.", "cursor-agent login"),
  ];
}

const SAMPLE: Record<string, string> = {
  claude: "**Start the garden.** It pays off for months, and a Saturday is enough to build one raised bed.\n\n- Plant fast growers: lettuce, radishes, herbs.\n- Bake bread on a rainy day instead.",
  gemini: "**Bake bread.** You get a result the same day, and the skills carry over to pizza and pastry.\n\n1. Start with a simple no-knead loaf.\n2. Save the garden for a long weekend.",
  openai: "Do the garden if the weather is good this weekend. If not, bread. Both are cheap to start.",
  grok: "Garden. Bread will still be there in winter.",
  glm: "Bread first: lower cost, faster feedback.",
  minimax: "A small herb garden, then bread with the herbs.",
};

export function mockApi(o: MockOptions = {}): Api & { calls: string[] } {
  const calls: string[] = [];
  const delay = o.delay ?? 0;
  const keys = new Set(o.keys ?? []);
  let settings: Settings = { ...defaultSettings(), onboarded: !!o.onboarded };
  let installed = !!o.onboarded;
  let history: HistoryEntry[] = [...(o.history ?? [])];
  const transcripts = new Map<string, string>(history.map((e) => [e.id, `# Round Table\n\n_Question:_ ${e.question}\n\n## Claude\n\nA saved answer.`]));
  const mode = o.claude ?? "both";
  const claude: ClaudeStatus = {
    app_found: mode === "app" || mode === "both" || mode === "plugin",
    app_connected: false,
    code_found: mode === "code" || mode === "both" || mode === "plugin",
    code_connected: false,
    plugin_ids: mode === "plugin" ? ["roundtable@frontier-infra"] : [],
    plugin_lines: ["/plugin marketplace add frontier-infra/roundtable", "/plugin install roundtable@frontier-infra"],
  };
  const tools: CliDetection[] = o.clis === "some" ? sampleClis() : Array.isArray(o.clis) ? o.clis.map((d) => ({ ...d })) : [];
  const wait = (ms = delay) => new Promise<void>((r) => setTimeout(r, ms));
  let stopped = false;
  let lastEmit: ((e: EngineEvent) => void) | null = null;
  const toolName = (h: string) => tools.find((t) => t.head === h)?.name;

  return {
    calls,
    appInfo: async () => ({ version: "0.1.1", os: o.os ?? "macos", arch: "aarch64" }),
    engineStatus: async () => ({ installed, current: installed, version: installed ? "0.1.1" : null, expected: "0.1.1" }),
    async engineSetup(onStep) {
      calls.push("engineSetup");
      await wait();
      onStep(0);
      await wait();
      if (o.setupFails) throw { code: "E_SELFTEST", step: 1, detail: "selftest failed" };
      onStep(1);
      await wait();
      onStep(2);
      installed = true;
    },
    async keysStatus(): Promise<KeysStatus> {
      const heads: KeysStatus["heads"] = {};
      for (const p of PROVIDERS) heads[p.id] = { provider: p.id, has_key: keys.has(p.id), source: keys.has(p.id) ? "keychain" : null };
      return { heads, importable: [] };
    },
    async keySave(head, key) {
      calls.push(`keySave:${head}`);
      await wait();
      if (key.includes("bad")) return { ok: false, reason: "bad_key", message: "That key did not work. Check you copied all of it." };
      if (key.includes("broke")) return { ok: false, reason: "no_credit", message: "That account has no credit left. Add credit on the provider's site." };
      keys.add(head);
      return { ok: true, message: "Saved." };
    },
    async keyRemove(head) {
      keys.delete(head);
    },
    keysImport: async () => ({ ok: true, imported: [] }),
    async clisDetect() {
      calls.push("clisDetect");
      return tools.map((d) => ({ ...d }));
    },
    async cliLogin(id) {
      calls.push(`cliLogin:${id}`);
      // The person finishes signing in in the terminal window.
      const t = tools.find((d) => d.id === id);
      if (t) t.signed_in = true;
      return { ok: true };
    },
    claudeStatus: async () => ({ ...claude }),
    async claudeConnectApp() {
      calls.push("claudeConnectApp");
      await wait();
      claude.app_connected = true;
      return { ok: true, message: "Done. Quit and reopen Claude to finish." };
    },
    async claudeConnectCode() {
      calls.push("claudeConnectCode");
      await wait();
      claude.code_connected = true;
      return { ok: true, message: "Done. Start a new Claude Code session to use it." };
    },
    async claudeDisablePlugin() {
      calls.push("claudeDisablePlugin");
      claude.plugin_ids = [];
      return { ok: true };
    },
    async claudeDisconnect() {
      claude.app_connected = claude.code_connected = false;
      return { ok: true };
    },
    async askStart(req, onEvent) {
      calls.push(`askStart:${req.heads.join(",")}:${req.rounds}`);
      stopped = false;
      const emit = (e: EngineEvent) => {
        if (!stopped) onEvent(e);
      };
      lastEmit = emit;
      const run = async () => {
        emit({ event: "run_started", heads: req.heads, rounds: req.rounds, mode: req.rounds > 1 ? "deliberation" : "advisory" });
        let answered = 0;
        const rounds = req.rounds > 1 ? 2 : 1;
        for (let r = 1; r <= rounds; r++) {
          for (const h of req.heads) emit({ event: "head_started", head: h, round: r });
          for (const h of req.heads) {
            await wait();
            const viaTool = settings.cli_heads.includes(h) && tools.some((t) => t.head === h && t.signed_in);
            const fail = o.runFails?.[h];
            const ok = !fail && (keys.has(h) || viaTool);
            if (ok) answered++;
            emit({
              event: "head_done",
              head: h,
              round: r,
              status: ok ? "answered" : "failed",
              text: ok ? (r > 1 ? `POSITION: ${SAMPLE[h]}` : SAMPLE[h] ?? "OK") : "",
              reason: ok || fail ? undefined : "bad_key",
              message: ok ? undefined : fail?.message ?? "That key did not work. Check you copied all of it.",
              fix: ok ? undefined : fail?.fix ?? "add_key",
              via: viaTool ? toolName(h) : keys.has(h) ? "API key" : undefined,
            });
          }
          emit({ event: "round_done", round: r });
          if (req.rounds > 1) {
            await wait();
            const consensus = r === rounds;
            emit({
              event: "chair_verdict",
              round: r,
              consensus,
              text: consensus
                ? "VERDICT: CONSENSUS\n**AGREED**\n- Start a small garden this Saturday.\n- Bake bread on the next rainy day.\n\n**REMAINING DISAGREEMENTS**\n- None of substance."
                : "VERDICT: CONTINUE\n**AGREED**\n- Both are cheap to start.\n\n**REMAINING DISAGREEMENTS**\n- Gemini prefers bread first; Claude prefers the garden.",
            });
            if (consensus) break;
          }
        }
        const id = `${Math.floor(Date.now() / 1000)}-abcd`;
        const transcript = `# Round Table\n\n_Question:_ ${req.question}\n`;
        if (settings.history) {
          history = [{ id, ts: Math.floor(Date.now() / 1000), question: req.question, source: "app", mode: req.rounds > 1 ? "deliberation" : "advisory", heads: req.heads }, ...history];
          transcripts.set(id, transcript);
        }
        emit({ event: "run_done", transcript, answered, consensus: req.rounds > 1 ? true : undefined, history_id: id });
        emit({ event: "process_exit" });
      };
      void run();
    },
    async askStop() {
      stopped = true;
    },
    async askRetry(head) {
      calls.push(`askRetry:${head}`);
      const emit = lastEmit;
      if (!emit) return;
      void (async () => {
        await wait();
        emit({ event: "head_started", head, round: 1 });
        await wait();
        emit({ event: "head_done", head, round: 1, status: "answered", text: SAMPLE[head] ?? "OK", via: toolName(head) ?? "API key" });
        emit({ event: "retry_done", head, transcript: `# Round Table\n\n---\n## ${head}\n\n${SAMPLE[head] ?? "OK"}\n\n---\n_footer_\n` });
      })();
    },
    async headTest(head, via): Promise<HeadTest> {
      calls.push(`headTest:${head}:${via}`);
      await wait();
      const fail = o.testFails?.[head];
      const name = toolName(head) ?? head;
      if (fail) return { head, name, ok: false, status: "failed", via: via === "key" ? "API key" : name, first_line: null, message: fail.message, fix: fail.fix, secs: 0.4, command: `${head} -p`, exit: 1 };
      return { head, name, ok: true, status: "answered", via: via === "key" ? "API key" : name, first_line: "Yes, water is wet.", message: null, fix: null, secs: 1.2, command: `${head} -p`, exit: 0 };
    },
    historyList: async () => [...history],
    historyGet: async (id) => transcripts.get(id) ?? null,
    async historyDelete(id) {
      history = history.filter((e) => e.id !== id);
    },
    async historyClear() {
      history = [];
    },
    settingsGet: async () => ({ ...settings }),
    async settingsSet(s) {
      settings = { ...s };
    },
    terminalCommand: async () => "Ready.",
    diagnostics: async () => "Roundtable 0.1.1\nOS: macos aarch64\n",
    async openUrl(url) {
      calls.push(`open:${url}`);
    },
    pickTextFile: async () => "/Users/someone/notes.md",
    saveTextFile: async () => true,
    async copyText(t) {
      calls.push(`copy:${t.length}`);
    },
    checkUpdate: async () => null,
    restart: async () => {},
  };
}
