// A fake backend with the same shape as the Rust core. Used by the UI tests and by the
// browser preview (`?mock=1`), so every screen can be seen without keys or network.

import type { Api, ClaudeStatus, CliDetection, EngineEvent, Fix, HeadTest, HistoryEntry, KeysStatus, LocalProvider, ModelInfo, Pack, Persona, ProviderConfig, ProviderRow, SeatInfo, Settings } from "./api";
import { defaultSettings, findPersona, flagOnly, labLabel, seatKeys, slug } from "./state";
import { builtinPacks, libraryOf } from "./mock-personas";
import { LOCAL_NOTE, PRESET_NAMES, PROVIDERS, splitSeat } from "./providers";

export interface MockOptions {
  /** A newer version on a channel (default stable). */
  update?: { version: string; notes: string; channel?: string };
  /** The app's version (default 0.1.1). */
  version?: string;
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
  /** Build without the Frontier labs pack. */
  noLabPack?: boolean;
  /** The person's own personas to start with. */
  custom?: Persona[];
  /** The coach's questions. Default: two. [] means the question is already clear. */
  coachQuestions?: string[];
  /** The coach (the chair's seat) does not answer. */
  coachFails?: boolean;
  /** Total memory for the Get Jebadiah hint. */
  memoryGb?: number | null;
  arch?: string;
  /** Our decision model is already installed. */
  jebadiahFound?: { app: string; model: string };
  /** `roundtable decision detect` found a Jebadiah server (jeb serve, AINode). */
  jebadiahServed?: { url: string; model: string; via: string };
  /** Ollama or LM Studio running on this computer. "ollama" is a sample Ollama. */
  localProviders?: "ollama" | LocalProvider[];
  /** Providers already added. */
  providers?: ProviderConfig[];
  /** Added providers whose key is saved, by id. */
  providerKeys?: string[];
}

const PRESET_URLS: Record<string, { url: string; key: boolean; page: string }> = {
  openrouter: { url: "https://openrouter.ai/api/v1", key: true, page: "https://openrouter.ai/settings/keys" },
  ollama: { url: "http://127.0.0.1:11434/v1", key: false, page: "" },
  lmstudio: { url: "http://127.0.0.1:1234/v1", key: false, page: "" },
  groq: { url: "https://api.groq.com/openai/v1", key: true, page: "https://console.groq.com/keys" },
  together: { url: "https://api.together.xyz/v1", key: true, page: "https://api.together.ai/settings/api-keys" },
  custom: { url: "", key: false, page: "" },
};

const isLocal = (url: string) => /^https?:\/\/(localhost|127\.|\[::1\]|0\.0\.0\.0)/i.test(url);

/** A sample Ollama with two small models. */
export function sampleOllama(): LocalProvider {
  return {
    preset: "ollama",
    name: "Ollama",
    base_url: "http://127.0.0.1:11434/v1",
    note: LOCAL_NOTE,
    models: [
      { id: "llama3.2:3b", free: true, ours: false, price_in: 0, price_out: 0 },
      { id: "qwen3:4b", free: true, ours: false, price_in: 0, price_out: 0 },
    ],
  };
}

/** What each preset lists in the preview. OpenRouter publishes prices; a stranger's Jebadiah is not ours. */
function sampleModels(preset: string, local: boolean): ModelInfo[] {
  const m = (id: string, price_in?: number, price_out?: number, note?: string): ModelInfo => ({ id, free: local, ours: false, price_in: local ? 0 : price_in, price_out: local ? 0 : price_out, note });
  if (preset === "openrouter")
    return [
      m("meta-llama/llama-3.3-70b-instruct", 0.12, 0.3),
      m("openai/gpt-4o-mini", 0.15, 0.6),
      m("openrouter/auto"),
      m("someone/jebadiah-7b", 0.1, 0.1, "Not a frontier-infra build. It sits as a plain seat only."),
    ];
  if (preset === "groq") return [m("llama-3.1-8b-instant"), m("qwen/qwen3-32b")];
  if (preset === "ollama" || preset === "lmstudio") return sampleOllama().models;
  return [m("small-a"), m("small-b")];
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

export function mockApi(o: MockOptions = {}): Api & { calls: string[]; menu(id: string): void } {
  const calls: string[] = [];
  let menuHandler: ((id: string) => void) | null = null;
  let downloaded: string | null = null;
  const delay = o.delay ?? 0;
  const keys = new Set(o.keys ?? []);
  let settings: Settings = { ...defaultSettings(), onboarded: !!o.onboarded };
  let installed = !!o.onboarded;
  let history: HistoryEntry[] = [...(o.history ?? [])];
  const transcripts = new Map<string, string>(history.map((e) => [e.id, `# Round Table\n\n_Question:_ ${e.question}\n\n## Claude\n\nA saved answer.`]));
  // Deleted roundtables waiting out the Undo window, like the engine's history/.trash.
  const trash = new Map<string, { rows: HistoryEntry[]; texts: [string, string][] }>();
  let trashN = 0;
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
  let builtins: Pack[] | null = null;
  const custom: Pack = { id: "custom", name: "My personas", description: "", personas: [...(o.custom ?? [])], presets: [], builtin: false };
  const library = async () => {
    builtins ??= await builtinPacks();
    return libraryOf([...builtins, ...(custom.personas.length ? [custom] : [])], settings.show_lab_pack, !o.noLabPack);
  };
  const freeId = async (want: string) => {
    const lib = await library();
    const used = (id: string) => lib.packs.some((p) => p.personas.some((x) => x.id === id)) || custom.personas.some((x) => x.id === id);
    const base = want || "persona";
    if (!used(base)) return base;
    for (let n = 2; ; n++) if (!used(`${base}-${n}`)) return `${base}-${n}`;
  };

  const found: LocalProvider[] = o.localProviders === "ollama" ? [sampleOllama()] : Array.isArray(o.localProviders) ? o.localProviders : [];
  settings.providers = (o.providers ?? []).map((p) => ({ ...p, models: [...p.models] }));
  const provKeys = new Set(o.providerKeys ?? []);
  const provRow = (c: ProviderConfig): ProviderRow => {
    const local = isLocal(c.base_url);
    const has = provKeys.has(c.id);
    return {
      id: c.id,
      preset: c.preset,
      name: c.name,
      base_url: c.base_url,
      needs_key: c.needs_key,
      has_key: has,
      key_source: has ? "keychain" : null,
      local,
      note: local ? LOCAL_NOTE : null,
      key_page: PRESET_URLS[c.preset]?.page ?? "",
      ready: !c.needs_key || has,
      models: c.models.map((x) => ({ ...x, seat: `${c.id}:${x.id}` })),
    };
  };
  const provOf = (seat: string) => {
    const sp = splitSeat(seat);
    return sp ? settings.providers?.find((p) => p.id === sp.provider) ?? null : null;
  };

  return {
    calls,
    async providersList() {
      calls.push("providersList");
      return {
        providers: (settings.providers ?? []).map(provRow),
        presets: Object.entries(PRESET_URLS).map(([id, v]) => ({ id, name: PRESET_NAMES[id], base_url: v.url, needs_key: v.key, local: !!v.url && isLocal(v.url), key_page: v.page })),
      };
    },
    async providersDetect() {
      calls.push("providersDetect");
      return { found: found.map((f) => ({ ...f, models: [...f.models] })) };
    },
    async providerModels(id) {
      calls.push(`providerModels:${id}`);
      await wait();
      const c = settings.providers?.find((p) => p.id === id);
      if (!c) return { ok: false, message: `No provider '${id}'.` };
      if (c.needs_key && !provKeys.has(id)) return { ok: false, reason: "add_key", message: `${c.name} needs a key.` };
      const local = isLocal(c.base_url);
      const models = found.find((f) => f.preset === c.preset)?.models ?? sampleModels(c.preset, local);
      return { ok: true, provider: id, local, total: models.length, models, ticked: c.models.map((m) => m.id) };
    },
    async providerAdd(req) {
      calls.push(`providerAdd:${req.preset}${req.models ? `:${req.models.join(",")}` : ""}${req.key ? ":key" : ""}`);
      await wait();
      const pre = PRESET_URLS[req.preset];
      if (!pre) return { ok: false, message: `Unknown provider '${req.preset}'.` };
      const id = req.id ?? (req.preset === "custom" ? slug(req.name || "custom") || "custom" : req.preset);
      const list = settings.providers ?? [];
      const cur = list.find((p) => p.id === id);
      const c: ProviderConfig = cur ? { ...cur, models: [...cur.models] } : { id, preset: req.preset, name: req.preset === "custom" ? req.name?.trim() || "Custom" : PRESET_NAMES[req.preset], base_url: pre.url, needs_key: pre.key, models: [] };
      if (req.url?.trim()) c.base_url = req.url.trim().replace(/\/+$/, "");
      if (!c.base_url) return { ok: false, message: "A custom provider needs its base URL: --url https://host/v1" };
      if (req.preset === "custom" && req.key) c.needs_key = true;
      if (req.check) {
        if (req.key?.includes("bad")) return { ok: false, reason: "bad_key", message: `${c.name} needs a working key. Check you copied all of it.` };
        if (c.needs_key && !req.key && !provKeys.has(id)) return { ok: false, reason: "bad_key", message: `${c.name} needs a working key. Check you copied all of it.` };
      }
      if (req.key) provKeys.add(id);
      if (req.models) {
        const listed = found.find((f) => f.preset === c.preset)?.models ?? sampleModels(c.preset, isLocal(c.base_url));
        c.models = req.models.map((m) => {
          const info = listed.find((x) => x.id === m);
          return { id: m, price_in: info?.price_in ?? null, price_out: info?.price_out ?? null };
        });
      }
      settings.providers = [...list.filter((p) => p.id !== id), c];
      return { ok: true, provider: provRow(c), seats: c.models.map((m) => `${id}:${m.id}`), message: `Saved ${c.name}.` };
    },
    async providerRemove(id) {
      calls.push(`providerRemove:${id}`);
      const c = settings.providers?.find((p) => p.id === id);
      if (!c) return { ok: false, message: `No provider '${id}'.` };
      settings.providers = (settings.providers ?? []).filter((p) => p.id !== id);
      settings.default_heads = settings.default_heads.filter((h) => !h.startsWith(`${id}:`));
      provKeys.delete(id);
      return { ok: true, message: `Removed ${c.name} and its ${c.models.length} seat${c.models.length === 1 ? "" : "s"}.` };
    },
    appInfo: async () => ({ version: o.version ?? "0.1.1", os: o.os ?? "macos", arch: "aarch64" }),
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
      calls.push(`askStart:${req.heads.join(",")}:${req.rounds}${req.personas ? `:${req.personas}` : ""}`);
      if (req.brief) calls.push(`brief:${req.brief}`);
      if (req.verdict) calls.push(`verdict:${req.verdict}`);
      if (req.openFirstRound) calls.push("open-first-round");
      if (req.prior) calls.push(`prior:${req.prior.length}`);
      if (req.attached) calls.push(`attached:${req.attached}`);
      const lib = await library();
      const picks = req.personas
        ? req.personas.split(",").map((x) => {
            const [pid, head] = x.split("@");
            return { persona: pid === "generic" ? "" : pid, head };
          })
        : req.heads.map((h) => ({ persona: "", head: h }));
      const keysOf = seatKeys(picks);
      const seats: SeatInfo[] = picks.map((s, i) => {
        const p = s.persona ? findPersona(lib, s.persona) : undefined;
        return { seat: keysOf[i], head: s.head, persona: p?.name, expertise: p?.expertise, label: p?.lab ? labLabel(p.lab) : undefined };
      });
      stopped = false;
      const emit = (e: EngineEvent) => {
        if (!stopped) onEvent(e);
      };
      lastEmit = emit;
      const run = async () => {
        emit({ event: "run_started", heads: req.heads, rounds: req.rounds, mode: req.rounds > 1 ? "deliberation" : "advisory", seats });
        // Clarify first: a question with no facts behind it (the preview asks about a database).
        if (seats.length > 1 && !req.brief && !req.prior && /database/i.test(req.question)) {
          emit({ event: "clarify", questions: ["What is the workload: reads, writes, and how much data?", "Which cloud and databases does your team already run?"] });
        }
        let answered = 0;
        const rounds = Math.max(1, req.rounds);
        for (let r = 1; r <= rounds; r++) {
          for (const st of seats) emit({ event: "head_started", head: st.head, round: r, seat: st.seat });
          for (const st of seats) {
            const h = st.head;
            const sample = SAMPLE[h] ?? (splitSeat(h) ? `Answer from ${splitSeat(h)!.model}.` : "OK");
            const said = st.persona ? `As ${st.persona}: ${sample}` : sample;
            await wait();
            const viaTool = settings.cli_heads.includes(h) && tools.some((t) => t.head === h && t.signed_in);
            const fail = o.runFails?.[h];
            const prov = provOf(h);
            const provOk = !!prov && (!prov.needs_key || provKeys.has(prov.id));
            const ok = !fail && (keys.has(h) || viaTool || provOk);
            if (ok) answered++;
            emit({
              event: "head_done",
              head: h,
              round: r,
              status: ok ? "answered" : "failed",
              seat: st.seat,
              text: ok ? (r > 1 ? `POSITION: ${said}${r > 2 ? ` (round ${r})` : ""}` : said) : "",
              reason: ok || fail ? undefined : "bad_key",
              message: ok ? undefined : fail?.message ?? "That key did not work. Check you copied all of it.",
              fix: ok ? undefined : fail?.fix ?? "add_key",
              via: prov ? prov.name : viaTool ? toolName(h) : keys.has(h) ? "API key" : undefined,
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
        if (req.verdict) {
          await wait();
          const texts: Record<string, string> = {
            decision:
              "DECISION: Start a small garden this Saturday.\nCONFIDENCE: Medium (65%)\nWOULD CHANGE IT: A week of rain in the forecast.\nNEXT STEPS:\n1. Measure the sunny corner today.\n2. Buy soil and seeds on Saturday at 9 am.\n3. Bake bread on the first rainy day.\n**WHY**\n- It pays off for months.\n- Bread fits a rainy day.",
            "pros-cons": "OPTION: Start the garden.\n**PROS**\n- Pays off for months.\n**CONS**\n- Weather can spoil the day.\nRECOMMENDATION: Garden now, bread when it rains.",
            plan: "GOAL: A raised bed by Sunday.\n**STEPS**\n1. You: buy soil and seeds, Saturday 9 am.\n2. You: build the bed, Saturday noon.\nFIRST STEP TODAY: Measure the sunny corner.",
            vote: "**VOTES**\n- Claude: garden\n- Gemini: bread\n- ChatGPT: garden\nRESULT: garden wins 2 to 1",
          };
          const minority = seats.length > 1 ? "Bake bread first: a result the same day." : "None";
          const heldBy = seats.length > 1 ? (seats[1].persona ? `${seats[1].persona} on ${seats[1].head}` : "Gemini") : "Nobody";
          emit({
            event: "verdict",
            shape: req.verdict,
            text: `${texts[req.verdict]}\nMINORITY VIEW: ${minority}\nHELD BY: ${heldBy}`,
            minority: minority === "None" ? undefined : minority,
            held_by: heldBy === "Nobody" ? undefined : heldBy,
            confidence: req.verdict === "decision" ? "medium" : undefined,
          });
        }
        const id = `${Math.floor(Date.now() / 1000)}-${(history.length + 0xabcd).toString(16)}`;
        const transcript = `# Round Table\n\n_Question:_ ${req.question}\n`;
        const keep = settings.history && req.source !== "setup" && !flagOnly(req.question);
        if (keep) {
          history = [{ id, ts: Math.floor(Date.now() / 1000), question: req.question, source: "app", mode: req.rounds > 1 ? "deliberation" : "advisory", heads: req.heads }, ...history];
          transcripts.set(id, transcript);
        }
        emit({ event: "run_done", transcript, answered, consensus: req.rounds > 1 ? true : undefined, history_id: keep ? id : undefined });
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
    async coachQuestions(question) {
      calls.push(`coachQuestions:${question}`);
      await wait();
      if (o.coachFails) return { ok: false, questions: [], message: "The coach did not answer. Connect Claude, or skip the coach." };
      return { ok: true, questions: o.coachQuestions ?? ["What matters more to you: a result today, or something that lasts?", "How much can you spend?"] };
    },
    async coachBrief(question, qa) {
      calls.push(`coachBrief:${qa.map((x) => x.a).join("|")}`);
      await wait();
      if (o.coachFails) return { ok: false, brief: { goal: question, context: "", constraints: "", good_answer: "" }, markdown: `- **Goal:** ${question}`, message: "The coach did not answer. Connect Claude, or skip the coach." };
      const said = qa.map((x) => x.a.trim()).filter(Boolean).join(" ");
      const brief = {
        goal: `Decide: ${question.replace(/\?$/, "")}.`,
        context: said || "None given",
        constraints: "None given",
        good_answer: "One clear pick, the main risk, and the first step.",
      };
      return { ok: true, brief, markdown: "" };
    },
    async suggestPanel(_question, _brief, heads) {
      calls.push(`suggestPanel:${heads.join(",")}`);
      await wait();
      const ids = ["cfo", "head-of-product", "customer-advocate"];
      const lib = await library();
      const placed = ids.map((id, i) => ({ id, head: heads[i % Math.max(1, heads.length)] ?? "claude" }));
      return {
        source: "chair",
        personas: ids,
        why: "Money, the product and the people who use it.",
        seats: placed.map((p) => ({ seat: `${p.id}@${p.head}`, head: p.head, persona: p.id, name: findPersona(lib, p.id)?.name ?? p.id })),
        spec: placed.map((p) => `${p.id}@${p.head}`).join(","),
      };
    },
    async attachPrepare(source) {
      calls.push(`attachPrepare:${source}`);
      await wait();
      if (source.includes("missing")) return { ok: false, message: "Could not find that file." };
      const link = /^https?:/.test(source);
      const name = link ? source : source.split(/[\\/]/).pop() ?? source;
      const pdf = name.toLowerCase().endsWith(".pdf");
      const long = pdf || name.includes("report");
      return {
        ok: true,
        attachments: [{ name, kind: link ? "link" : pdf ? "pdf" : "text", chars: long ? 48210 : 1840, shared: long ? "summary" : "full", text: long ? "- Budget is 40 thousand.\n- Launch in March." : "Notes: budget 40 thousand, launch in March." }],
      };
    },
    pickAttachment: async () => "/Users/someone/plan.pdf",
    personasList: library,
    async personaSave(p) {
      calls.push(`personaSave:${p.name}`);
      if (!p.name.trim()) throw "Give the persona a name.";
      const i = custom.personas.findIndex((x) => x.id === p.id);
      const saved = { ...p, name: p.name.trim(), id: i >= 0 ? p.id : await freeId(slug(p.id || p.name)) };
      if (i >= 0) custom.personas[i] = saved;
      else custom.personas.push(saved);
      return saved;
    },
    async personaDuplicate(id) {
      calls.push(`personaDuplicate:${id}`);
      const src = findPersona(await library(), id);
      if (!src) throw "That persona is gone.";
      const copy = { ...src, name: `${src.name} (copy)`, id: await freeId(slug(`${src.name} copy`)) };
      custom.personas.push(copy);
      return copy;
    },
    async personaDelete(id) {
      calls.push(`personaDelete:${id}`);
      const n = custom.personas.length;
      custom.personas = custom.personas.filter((x) => x.id !== id);
      if (custom.personas.length === n) throw "Only your own personas can be deleted.";
    },
    async personasExport() {
      calls.push("personasExport");
      return custom.personas.length;
    },
    async personasImport() {
      calls.push("personasImport");
      custom.personas.push({ id: await freeId("ops-lead"), name: "Ops lead", expertise: "Uptime and on-call", brief: "Count the nines. Ask who gets paged." });
      return 1;
    },
    historyList: async () => [...history],
    historyGet: async (id) => transcripts.get(id) ?? null,
    async historyDelete(ids) {
      calls.push(`historyDelete:${ids.join(",")}`);
      const token = `${Date.now()}-${(trashN++).toString(16)}`;
      const rows = history.filter((e) => ids.includes(e.id));
      trash.set(token, { rows, texts: ids.filter((i) => transcripts.has(i)).map((i) => [i, transcripts.get(i)!]) });
      history = history.filter((e) => !ids.includes(e.id));
      ids.forEach((i) => transcripts.delete(i));
      return token;
    },
    async historyRestore(token) {
      calls.push(`historyRestore:${token}`);
      const t = trash.get(token);
      if (!t) throw new Error("already deleted");
      trash.delete(token);
      history = [...history, ...t.rows].sort((a, b) => b.ts - a.ts || b.id.localeCompare(a.id));
      t.texts.forEach(([i, x]) => transcripts.set(i, x));
      return t.rows.length;
    },
    async historyPurge(token) {
      calls.push(`historyPurge:${token}`);
      trash.delete(token);
    },
    async historyClear() {
      calls.push("historyClear");
      history = [];
      transcripts.clear();
      trash.clear();
    },
    settingsGet: async () => ({ ...settings, providers: (settings.providers ?? []).map((p) => ({ ...p, models: [...p.models] })) }),
    async settingsSet(s) {
      // As the core does: the engine owns the added providers.
      settings = { ...s, providers: settings.providers };
    },
    terminalCommand: async () => "Ready.",
    diagnostics: async () => "Roundtable 0.1.1\nOS: macos aarch64\n",
    async openUrl(url) {
      calls.push(`open:${url}`);
    },
    async onMenu(handler) {
      menuHandler = handler;
    },
    menu(id: string) {
      calls.push(`menu:${id}`);
      menuHandler?.(id);
    },
    async jebadiahLocal() {
      calls.push("jebadiahLocal");
      return { os: o.os ?? "macos", arch: o.arch ?? "aarch64", memory_gb: o.memoryGb === undefined ? 32 : o.memoryGb, found: o.jebadiahFound ?? null, served: o.jebadiahServed ?? null, hint: null, looked: [] };
    },
    pickTextFile: async () => "/Users/someone/notes.md",
    async saveTextFile(name, t) {
      calls.push(`save:${name}:${t}`);
      return true;
    },
    async copyText(t) {
      calls.push(`copy:${t.length}`);
    },
    async updateCheck(channel) {
      calls.push(`updateCheck:${channel}`);
      const u = o.update && (o.update.channel ?? "stable") === channel ? o.update : null;
      if (!u) return null;
      return { version: u.version, current: "0.1.1", notes: u.notes, date: null, channel, ready: downloaded === u.version };
    },
    async updateDownload(installOnQuit) {
      calls.push(`updateDownload:${installOnQuit}`);
      if (o.update) downloaded = o.update.version;
    },
    async updateOnQuit(on) {
      calls.push(`updateOnQuit:${on}`);
    },
    async updateInstall() {
      calls.push("updateInstall");
    },
  };
}
