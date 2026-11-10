// A fake backend with the same shape as the Rust core. Used by the UI tests and by the
// browser preview (`?mock=1`), so every screen can be seen without keys or network.

import type { Api, ClaudeStatus, CliDetection, EngineEvent, Fix, HeadTest, HistoryEntry, KeysStatus, Pack, Persona, SeatInfo, Settings } from "./api";
import { defaultSettings, findPersona, labLabel, seatKeys, slug } from "./state";
import { builtinPacks, libraryOf } from "./mock-personas";
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
            const said = st.persona ? `As ${st.persona}: ${SAMPLE[h] ?? "OK"}` : SAMPLE[h] ?? "OK";
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
              seat: st.seat,
              text: ok ? (r > 1 ? `POSITION: ${said}${r > 2 ? ` (round ${r})` : ""}` : said) : "",
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
    checkUpdate: async () => null,
    restart: async () => {},
  };
}
