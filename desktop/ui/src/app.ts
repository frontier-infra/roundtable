// The controller: holds state, runs actions against the backend, re-renders.

import { errorText, type Api, type EngineEvent, type HeadId, type VerdictShape } from "./api";
import { attachmentLabel, attachmentsBlock, briefMarkdown, coachText, emptyBrief, looksLikeLink, mdFileName, priorText, runMarkdown, runSeats, sessionMarkdown } from "./session";
import {
  applyEvent,
  back,
  cliHeadsOn,
  connectedHeads,
  defaultCliHeads,
  initialState,
  keyCardFromResult,
  keysFromStatus,
  newRun,
  next,
  planSeats,
  roundsOf,
  seatInfos,
  seatSpec,
  tableHeads,
  TEST_TRIES,
  type Mode,
  type SeatPick,
  type State,
  type Step,
} from "./state";
import { renderOnboarding } from "./views/onboarding";
import { renderMain } from "./views/main";
import { renderSettings } from "./views/settings";
import { h } from "./dom";
import { HEAD_ORDER } from "./providers";

const UPDATE_EVERY_MS = 6 * 60 * 60 * 1000;

/** What a run reads besides the question. */
export interface AskExtra {
  brief?: string | null;
  verdict?: VerdictShape | null;
  openFirstRound?: boolean;
  prior?: string | null;
  attached?: string | null;
  attachments?: string[];
  target?: string;
  /** Ask only these AIs (a follow-up to one seat). */
  onlyHeads?: string[];
}

export class App {
  state: State = initialState();
  private updateInstall: (() => Promise<void>) | null = null;
  private toastTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    public api: Api,
    private root: HTMLElement,
  ) {}

  /** Load what we know, then pick the first screen. */
  async start(opts: { screen?: number; view?: State["view"] } = {}) {
    try {
      const info = await this.api.appInfo();
      this.state.os = info.os;
      this.state.version = info.version;
      if (info.start === "settings") opts = { ...opts, view: "settings" };
      else if (info.start && Number(info.start) >= 1) opts = { ...opts, screen: Number(info.start) };
    } catch {
      /* keep defaults */
    }
    try {
      this.state.settings = await this.api.settingsGet();
      const saved = this.state.settings.personas ?? "";
      if (saved.startsWith("preset:") || saved.startsWith("pack:")) this.state.ask = { ...this.state.ask, seats: saved };
      this.state.ask = { ...this.state.ask, coach: this.state.settings.coach !== false, verdict: this.state.settings.verdict_shape || "decision" };
    } catch {
      /* defaults */
    }
    try {
      const es = await this.api.engineStatus();
      this.state.engineReady = es.installed && es.current;
      if (this.state.engineReady) this.state.setup = { status: "done", ticks: 3, attempts: 0 };
    } catch {
      /* not set up */
    }
    await this.refreshKeys();
    // Look for Claude Code, Codex and the rest in the background. No prompt is sent.
    void this.refreshClis();
    // Back from a sign-in window: look again.
    if (typeof window !== "undefined") {
      window.addEventListener("focus", () => {
        if (this.state.clis.signingIn.length && !this.state.clis.loading) void this.refreshClis();
      });
    }
    if (opts.view) this.state.view = opts.view;
    else this.state.view = this.state.settings.onboarded && this.state.engineReady ? "main" : "onboarding";
    if (opts.screen) this.state.step = Math.min(5, Math.max(1, opts.screen)) as Step;
    if (this.state.view !== "onboarding") await Promise.all([this.refreshHistory(), this.refreshPersonas()]);
    if (this.state.step === 4) void this.refreshClaude();
    this.render();
    if (this.state.settings.auto_update) {
      void this.checkUpdate(false);
      setInterval(() => void this.checkUpdate(false), UPDATE_EVERY_MS);
    }
  }

  set(patch: Partial<State>) {
    Object.assign(this.state, patch);
    this.render();
  }

  render() {
    const s = this.state;
    const view = s.view === "onboarding" ? renderOnboarding(this) : s.view === "settings" ? renderSettings(this) : renderMain(this);
    const banner = s.update
      ? h(
          "div",
          { class: "banner", role: "status" },
          h("span", { text: "An update is ready." }),
          h("button", { class: "btn small primary", type: "button", text: "Restart now", on: { click: () => void this.installUpdate() } }),
          h("button", { class: "btn small", type: "button", text: "Later", on: { click: () => this.set({ update: null }) } }),
        )
      : null;
    const toast = s.toast ? h("div", { class: "toast", role: "status", text: s.toast }) : null;
    this.root.replaceChildren(...[banner, view, toast].filter((x): x is HTMLElement => !!x));
  }

  showToast(msg: string) {
    this.state.toast = msg;
    this.render();
    if (this.toastTimer) clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => this.set({ toast: null }), 3000);
  }

  // ---- Walkthrough ----

  goNext() {
    const step = next(this.state.step);
    this.set({ step });
    if (step === 3) void this.testTickedClis();
    if (step === 4) void this.refreshClaude();
  }

  goBack() {
    this.set({ step: back(this.state.step) });
  }

  async runSetup() {
    const attempts = this.state.setup.attempts + 1;
    this.set({ setup: { status: "working", ticks: 0, attempts } });
    try {
      await this.api.engineSetup((i) => {
        this.state.setup = { ...this.state.setup, ticks: Math.max(this.state.setup.ticks, i + 1) };
        this.render();
      });
      this.set({ setup: { status: "done", ticks: 3, attempts }, engineReady: true });
      await this.refreshKeys();
    } catch (e) {
      const f = e as { code?: string; detail?: string };
      this.set({ setup: { status: "failed", ticks: this.state.setup.ticks, attempts, error: { code: f.code ?? "E_UNKNOWN", detail: f.detail ?? errorText(e) } } });
    }
  }

  async refreshKeys() {
    try {
      const st = await this.api.keysStatus();
      this.state.keys = keysFromStatus(st, this.state.os, this.state.keys);
      this.state.importable = st.importable ?? [];
    } catch {
      /* engine not ready yet */
    }
  }

  async saveKey(head: HeadId, key: string) {
    if (!key.trim()) return;
    this.state.keys = { ...this.state.keys, [head]: { status: "checking" } };
    this.render();
    try {
      const r = await this.api.keySave(head, key);
      this.state.keys = { ...this.state.keys, [head]: keyCardFromResult(r, this.state.os) };
    } catch (e) {
      this.state.keys = { ...this.state.keys, [head]: { status: "error", message: errorText(e) } };
    }
    this.render();
  }

  async removeKey(head: HeadId) {
    try {
      await this.api.keyRemove(head);
    } catch (e) {
      this.showToast(errorText(e));
    }
    this.state.keys = { ...this.state.keys, [head]: { status: "empty" } };
    await this.refreshKeys();
    this.render();
  }

  async importKeys(remove: boolean) {
    try {
      const r = await this.api.keysImport(remove);
      await this.refreshKeys();
      this.showToast(r.imported?.length ? `Moved ${r.imported.length} key${r.imported.length === 1 ? "" : "s"} to ${this.state.os === "windows" ? "Credential Manager" : "the Keychain"}.` : "No keys to move.");
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  // ---- AI tools on this computer ----

  async refreshClis() {
    const before = this.state.clis;
    this.state.clis = { ...before, loading: true };
    this.render();
    try {
      const list = await this.api.clisDetect();
      const nowIn = (id: string) => list.find((d) => d.id === id)?.signed_in === true;
      const justSignedIn = before.signingIn.filter(nowIn).map((id) => list.find((d) => d.id === id)!.head);
      this.state.clis = { loaded: true, loading: false, list, signingIn: before.signingIn.filter((id) => !nowIn(id)) };
      const cur = this.state.settings.cli_heads ?? [];
      let next = cur;
      if (!this.state.settings.clis_chosen) next = defaultCliHeads(list);
      else if (justSignedIn.length) next = HEAD_ORDER.filter((h) => cur.includes(h) || justSignedIn.includes(h));
      if (next.join(",") !== cur.join(",")) await this.saveSettings({ cli_heads: next });
      // A tool that just finished signing in gets a fresh test.
      for (const h of justSignedIn) delete this.state.tests[`app:${h}`];
      if (this.state.view === "onboarding" && this.state.step === 3) void this.testTickedClis();
    } catch (e) {
      this.state.clis = { ...before, loaded: true, loading: false, error: errorText(e) };
    }
    this.render();
  }

  async setCli(head: string, on: boolean) {
    const set = new Set(this.state.settings.cli_heads ?? []);
    if (on) set.add(head);
    else set.delete(head);
    // Ticking again clears an old failure, and setup checks it once more.
    if (on) delete this.state.tests[`app:${head}`];
    await this.saveSettings({ cli_heads: HEAD_ORDER.filter((h) => set.has(h)), clis_chosen: true });
    if (on && this.state.view === "onboarding") void this.testHead(head, "app", true);
  }

  // ---- Five-word tests ----

  /** Setup: test every ticked, signed-in tool that has not been tested yet. */
  async testTickedClis() {
    if (!this.state.clis.loaded) return;
    const ticked = (this.state.settings.cli_heads ?? []).filter((h) =>
      this.state.clis.list.some((d) => d.head === h && d.found && d.signed_in !== false),
    );
    await Promise.all(ticked.filter((h) => !this.state.tests[`app:${h}`]).map((h) => this.testHead(h, "app", true)));
  }

  /** Ask one AI a five-word question. In setup (`auto`), a tool that fails twice is unticked. */
  async testHead(head: string, via: "auto" | "app" | "key", auto = false): Promise<void> {
    const k = `${via === "key" ? "key" : "app"}:${head}`;
    const prev = this.state.tests[k];
    if (prev?.status === "testing") return;
    const failures = prev?.status === "failed" ? prev.failures : 0;
    this.state.tests = { ...this.state.tests, [k]: { status: "testing", failures } };
    this.render();
    let ok = false;
    let result;
    try {
      result = await this.api.headTest(head, via);
      ok = result.ok;
    } catch (e) {
      result = { head, name: head, ok: false, status: "failed", via: null, first_line: null, message: errorText(e), fix: "retry" as const, secs: 0, command: null, exit: null };
    }
    if (ok) {
      this.state.tests = { ...this.state.tests, [k]: { status: "ok", failures: 0, result } };
      this.render();
      return;
    }
    const n = failures + 1;
    this.state.tests = { ...this.state.tests, [k]: { status: "failed", failures: n, result } };
    this.render();
    if (!auto) return;
    // One more try first: a slow start or a busy server often passes the second time.
    if (n < TEST_TRIES) return this.testHead(head, via, true);
    if (via !== "key") {
      this.state.tests = { ...this.state.tests, [k]: { status: "failed", failures: TEST_TRIES, result, unticked: true } };
      const set = new Set(this.state.settings.cli_heads ?? []);
      set.delete(head);
      await this.saveSettings({ cli_heads: HEAD_ORDER.filter((h) => set.has(h)), clis_chosen: true });
    }
  }

  /** Ask one failed seat of a finished one-round run again, on its own. `seat` is the seat key. */
  async retryHead(seat: string) {
    const run = this.state.run;
    if (!run) return;
    const head = run.cards[seat]?.head ?? run.seats[seat]?.head ?? seat;
    const persona = seat.includes("@") ? seat.slice(0, seat.indexOf("@")) : undefined;
    this.state.run = applyEvent(run, { event: "head_started", head, round: 1, seat });
    this.render();
    try {
      await this.api.askRetry(head, { question: run.question, research: !!run.research, contextPath: run.context ?? null, seat, persona, brief: run.brief ?? null, prior: run.prior ?? null, attached: run.attached ?? null });
    } catch (e) {
      this.state.run = applyEvent(this.state.run!, { event: "head_done", head, round: 1, status: "failed", text: "", message: errorText(e), fix: "retry", seat });
      this.render();
    }
  }

  /** The button on a card that did not answer. `seat` is the seat key (the head id on a Generic seat). */
  async applyFix(seat: string, fix: string) {
    if (fix === "retry") return this.retryHead(seat);
    const head = this.state.run?.cards[seat]?.head ?? seat;
    if (fix === "sign_in") {
      const d = this.state.clis.list.find((x) => x.head === head);
      if (d) return this.cliSignIn(d.id);
    }
    if (fix === "add_key" || fix === "sign_in") {
      if (this.state.view === "onboarding") this.set({ step: 3, run: null, keysOpen: true });
      else await this.openSettings();
    }
  }

  async cliSignIn(id: string) {
    try {
      await this.api.cliLogin(id);
      const signingIn = [...new Set([...this.state.clis.signingIn, id])];
      this.state.clis = { ...this.state.clis, signingIn };
    } catch (e) {
      this.showToast(errorText(e));
    }
    this.render();
  }

  cliHeads(): string[] {
    return cliHeadsOn(this.state);
  }

  async refreshClaude() {
    this.state.claude = { ...this.state.claude, loading: true };
    this.render();
    try {
      const st = await this.api.claudeStatus();
      this.state.claude = { ...this.state.claude, ...st, loading: false, error: undefined };
    } catch (e) {
      this.state.claude = { ...this.state.claude, loading: false, error: errorText(e) };
    }
    this.render();
  }

  async connectClaude(which: "app" | "code" | "plugin") {
    this.state.claude = { ...this.state.claude, busy: which, error: undefined };
    this.render();
    try {
      const r =
        which === "app" ? await this.api.claudeConnectApp() : which === "code" ? await this.api.claudeConnectCode() : await this.api.claudeDisablePlugin();
      const c = { ...this.state.claude, busy: undefined };
      if (which === "app") Object.assign(c, { app_connected: true, appMessage: r.message ?? "Done. Quit and reopen Claude to finish." });
      if (which === "code") Object.assign(c, { code_connected: true, codeMessage: r.message ?? "Done. Start a new Claude Code session to use it." });
      if (which === "plugin") Object.assign(c, { plugin_ids: [], pluginMessage: "Done. Claude now uses the app's engine." });
      this.state.claude = c;
    } catch (e) {
      this.state.claude = { ...this.state.claude, busy: undefined, error: errorText(e) };
    }
    this.render();
  }

  async disconnectClaude() {
    try {
      await this.api.claudeDisconnect();
      this.showToast("Disconnected from Claude.");
    } catch (e) {
      this.showToast(errorText(e));
    }
    this.state.claude = { loading: true };
    await this.refreshClaude();
  }

  // ---- Asking ----

  /** Start a roundtable. Used by screen 5 and the main screen. `rounds` is 1 to 4
   * ("quick" is 1, "talk" is 2). `seats` defaults to the ask box's Seats choice. */
  async ask(question: string, rounds: number | Mode, research: boolean, context?: { path?: string | null; text?: string | null }, seats?: SeatPick[], extra: AskExtra = {}) {
    const all = tableHeads(this.state.settings, this.state.keys, this.cliHeads());
    // A follow-up to one seat asks only that seat's AI.
    const heads = extra.onlyHeads?.length ? all.filter((h) => extra.onlyHeads!.includes(h)) : all;
    if (!question.trim() || !heads.length) return;
    const ctx = context?.text ? "follow-up" : context?.path ?? null;
    const n = roundsOf(rounds);
    const plan = seats ?? (this.state.view === "onboarding" ? heads.map((h) => ({ persona: "", head: h })) : planSeats(this.state.ask, heads, this.state.personas));
    const personas = seatSpec(plan, heads);
    const infos = personas ? seatInfos(plan, this.state.personas) : undefined;
    const verdict = extra.verdict === undefined ? null : extra.verdict;
    this.state.run = {
      ...newRun(question.trim(), heads, n, research, ctx, infos),
      brief: extra.brief ?? null,
      verdictShape: verdict,
      openFirstRound: !!extra.openFirstRound,
      attachments: extra.attachments ?? [],
      target: extra.target ?? "table",
      prior: extra.prior ?? null,
      attached: extra.attached ?? null,
    };
    this.state.opened = null;
    this.state.coach = null;
    this.render();
    const onEvent = (e: EngineEvent) => {
      if (!this.state.run || this.state.run.stopped) return;
      this.state.run = applyEvent(this.state.run, e);
      this.render();
      // After the run, a tool that answered counts as tested.
      if (e.event === "head_done" && e.status === "answered" && e.via && e.via !== "API key") {
        const k = `app:${e.head}`;
        if (this.state.tests[k]?.status === "failed") delete this.state.tests[k];
      }
      if (e.event === "run_done") void this.refreshHistory().then(() => this.render());
    };
    try {
      await this.api.askStart(
        {
          question: question.trim(),
          heads,
          rounds: n,
          research,
          contextPath: context?.path ?? null,
          contextText: context?.text ?? null,
          personas,
          brief: extra.brief ?? null,
          verdict,
          openFirstRound: !!extra.openFirstRound,
          prior: extra.prior ?? null,
          attached: extra.attached ?? null,
        },
        onEvent,
      );
    } catch (e) {
      this.state.run = { ...this.state.run, done: true };
      this.showToast(errorText(e));
    }
  }

  async stop() {
    await this.api.askStop();
    if (this.state.run) this.state.run = { ...this.state.run, done: true, stopped: true };
    this.render();
  }

  newQuestion() {
    this.state.run = null;
    this.state.opened = null;
    this.state.coach = null;
    this.state.session = [];
    this.state.followUpText = "";
    this.state.ask = { ...this.state.ask, question: "", contextPath: null, contextText: null, contextName: null, attachments: [], link: "" };
    this.render();
  }

  /** The ask box's main button: the coach first when it is on, else straight to the table. */
  async startAsk() {
    const a = this.state.ask;
    if (!a.question.trim()) return;
    if (a.coach) return this.openCoach(a.question.trim());
    return this.askTable(a.question.trim(), null);
  }

  /** Send the ask box's question to the table, with a brief from the coach or none. */
  async askTable(question: string, brief: string | null, seats?: SeatPick[]) {
    const a = this.state.ask;
    const heads = tableHeads(this.state.settings, this.state.keys, this.cliHeads());
    await this.ask(question, a.rounds, a.research, { path: a.contextPath, text: a.contextText }, seats ?? planSeats(a, heads, this.state.personas), {
      brief,
      verdict: a.verdict,
      openFirstRound: this.state.settings.blind_first_round === false,
      attached: attachmentsBlock(a.attachments) || null,
      attachments: a.attachments.map(attachmentLabel),
      prior: a.contextText ? null : this.state.priorFromHistory,
    });
    this.state.priorFromHistory = null;
  }

  // ---- The question coach ----

  async openCoach(question: string) {
    this.state.coach = { stage: "questions", loading: true, question, questions: [], answers: [], brief: emptyBrief(question), seats: null, suggesting: false, useSuggested: false };
    this.render();
    try {
      const r = await this.api.coachQuestions(coachText(question, this.state.ask.attachments));
      const c = this.state.coach;
      if (!c || c.question !== question) return;
      if (!r.ok) {
        this.state.coach = { ...c, loading: false, error: r.message ?? "The coach did not answer. Connect Claude, or skip the coach." };
      } else if (!r.questions.length) {
        // Already clear: straight to the brief.
        this.state.coach = { ...c, loading: false };
        return this.writeBrief();
      } else {
        this.state.coach = { ...c, loading: false, questions: r.questions.slice(0, 3), answers: r.questions.slice(0, 3).map(() => "") };
      }
    } catch (e) {
      if (this.state.coach) this.state.coach = { ...this.state.coach, loading: false, error: errorText(e) };
    }
    this.render();
  }

  async writeBrief() {
    const c = this.state.coach;
    if (!c) return;
    this.state.coach = { ...c, loading: true, error: undefined };
    this.render();
    try {
      const r = await this.api.coachBrief(coachText(c.question, this.state.ask.attachments), c.questions.map((q, i) => ({ q, a: c.answers[i] ?? "" })));
      if (!this.state.coach) return;
      this.state.coach = { ...this.state.coach, stage: "brief", loading: false, brief: r.brief, error: r.ok ? undefined : r.message };
    } catch (e) {
      if (this.state.coach) this.state.coach = { ...this.state.coach, stage: "brief", loading: false, error: errorText(e) };
    }
    this.render();
    void this.suggestSeats();
  }

  /** Propose seats from the brief. The person accepts or changes them. */
  async suggestSeats() {
    const c = this.state.coach;
    if (!c) return;
    const heads = tableHeads(this.state.settings, this.state.keys, this.cliHeads());
    this.state.coach = { ...c, suggesting: true };
    this.render();
    try {
      const r = await this.api.suggestPanel(c.question, briefMarkdown(c.brief), heads);
      if (!this.state.coach) return;
      const seats = r.seats.filter((x) => x.persona).map((x) => ({ persona: x.persona ?? "", head: x.head }));
      // Suggested seats start chosen unless the person already picked seats in the ask box.
      this.state.coach = { ...this.state.coach, suggesting: false, suggestion: r, seats, useSuggested: seats.length > 0 && !this.state.ask.seats };
    } catch {
      if (this.state.coach) this.state.coach = { ...this.state.coach, suggesting: false, seats: [] };
    }
    this.render();
  }

  /** The coach's last button: the brief (as edited) and the chosen seats go to the table. */
  async acceptBrief() {
    const c = this.state.coach;
    if (!c) return;
    const md = briefMarkdown(c.brief);
    const seats = c.useSuggested && c.seats?.length ? c.seats : undefined;
    await this.askTable(c.question, md || null, seats);
  }

  skipCoach() {
    const c = this.state.coach;
    if (!c) return;
    void this.askTable(c.question, null);
  }

  closeCoach() {
    this.set({ coach: null });
  }

  // ---- Files and links every seat reads ----

  async addAttachment(source: string) {
    const src = source.trim();
    if (!src) return;
    if (this.state.ask.attachments.some((a) => a.name === src || src.endsWith(`/${a.name}`) || src.endsWith(`\\${a.name}`))) {
      this.showToast("That one is already attached.");
      return;
    }
    this.state.ask = { ...this.state.ask, attaching: true };
    this.render();
    try {
      const r = await this.api.attachPrepare(src);
      if (!r.ok || !r.attachments?.length) throw r.message ?? "Could not read that.";
      this.state.ask = { ...this.state.ask, attaching: false, link: looksLikeLink(src) ? "" : this.state.ask.link, attachments: [...this.state.ask.attachments, ...r.attachments] };
    } catch (e) {
      this.state.ask = { ...this.state.ask, attaching: false };
      this.showToast(errorText(e));
    }
    this.render();
  }

  async pickAttachment() {
    const p = await this.api.pickAttachment();
    if (p) await this.addAttachment(p);
  }

  async addLink() {
    const l = this.state.ask.link.trim();
    if (!looksLikeLink(l)) {
      this.showToast("Paste a full link that starts with https://");
      return;
    }
    await this.addAttachment(l);
  }

  removeAttachment(i: number) {
    this.set({ ask: { ...this.state.ask, attachments: this.state.ask.attachments.filter((_, j) => j !== i) } });
  }

  // ---- Follow-ups ----

  /** Ask a follow-up of the whole table or of one seat, keeping the session's history. */
  async askFollowUp(question: string, target: string) {
    const run = this.state.run;
    if (!run || !run.done || !question.trim()) return;
    const turns = [...this.state.session, run];
    this.state.session = turns;
    this.state.followUpText = "";
    const prior = priorText(turns);
    const a = this.state.ask;
    const shared = { attached: attachmentsBlock(a.attachments) || null, attachments: a.attachments.map(attachmentLabel), prior };
    if (target !== "table") {
      const seat = runSeats(run).find((_, i) => run.heads[i] === target);
      if (!seat) return;
      await this.ask(question, 1, !!run.research, undefined, [seat], { ...shared, verdict: null, target, onlyHeads: [seat.head] });
      return;
    }
    // The whole table: the seats, rounds and verdict of the last turn that asked the whole table.
    const table = [...turns].reverse().find((t) => (t.target ?? "table") === "table") ?? run;
    await this.ask(question, table.rounds, !!table.research, undefined, runSeats(table), {
      ...shared,
      brief: null,
      verdict: table.verdictShape ?? a.verdict,
      openFirstRound: this.state.settings.blind_first_round === false,
      target: "table",
    });
  }

  /** "Ask a follow-up" on a past roundtable: its transcript is the history. */
  followUp() {
    const transcript = this.state.run?.transcript || this.state.opened?.transcript || "";
    this.state.run = null;
    this.state.opened = null;
    this.state.priorFromHistory = transcript || null;
    this.state.ask = { ...this.state.ask, question: "", contextPath: null, contextText: null, contextName: transcript ? "The last roundtable" : null };
    this.render();
  }

  // ---- Markdown ----

  async copyMarkdown(run = this.state.run) {
    if (!run) return;
    try {
      await this.api.copyText(runMarkdown(run));
      this.showToast("Copied as Markdown.");
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async downloadMarkdown(run = this.state.run) {
    if (!run) return;
    try {
      if (await this.api.saveTextFile(mdFileName(run.question), runMarkdown(run))) this.showToast("Saved.");
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  /** Every turn of this session, as one Markdown file. */
  async exportSession() {
    const turns = [...this.state.session, ...(this.state.run ? [this.state.run] : [])];
    if (!turns.length) return;
    try {
      if (await this.api.saveTextFile(mdFileName(turns[0].question, true), sessionMarkdown(turns))) this.showToast("Session saved.");
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async finishOnboarding() {
    const settings = { ...this.state.settings, onboarded: true };
    try {
      await this.api.settingsSet(settings);
    } catch {
      /* still continue */
    }
    this.state.settings = settings;
    this.state.run = null;
    this.state.view = "main";
    await Promise.all([this.refreshHistory(), this.refreshPersonas()]);
    this.render();
  }

  // ---- History ----

  async refreshHistory() {
    try {
      this.state.history = await this.api.historyList();
    } catch {
      this.state.history = [];
    }
  }

  async openHistory(id: string) {
    const t = await this.api.historyGet(id);
    if (t === null) {
      this.showToast("That roundtable is no longer on this computer.");
      await this.refreshHistory();
      this.render();
      return;
    }
    this.state.run = null;
    this.state.opened = { id, transcript: t };
    this.render();
  }

  async deleteHistory(id: string) {
    await this.api.historyDelete(id);
    if (this.state.opened?.id === id) this.state.opened = null;
    await this.refreshHistory();
    this.render();
  }

  async clearHistory() {
    await this.api.historyClear();
    this.state.opened = null;
    await this.refreshHistory();
    this.showToast("History deleted.");
  }

  // ---- Personas ----

  async refreshPersonas() {
    try {
      this.state.personas = await this.api.personasList();
    } catch {
      /* engine not ready: the Seats control offers Generic only */
    }
  }

  /** Save a persona from the editor. A new one has an empty id. */
  async savePersona(p: import("./api").Persona) {
    try {
      const saved = await this.api.personaSave(p);
      this.state.personaEdit = null;
      await this.refreshPersonas();
      this.showToast(`Saved ${saved.name}.`);
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async duplicatePersona(id: string) {
    try {
      const copy = await this.api.personaDuplicate(id);
      await this.refreshPersonas();
      this.state.personaEdit = copy;
      this.render();
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async deletePersona(id: string) {
    try {
      await this.api.personaDelete(id);
      if (this.state.personaEdit?.id === id) this.state.personaEdit = null;
      await this.refreshPersonas();
      this.render();
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async exportPersonas() {
    try {
      const n = await this.api.personasExport();
      if (n !== null) this.showToast(`Saved ${n} persona${n === 1 ? "" : "s"} to the file.`);
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async importPersonas() {
    try {
      const n = await this.api.personasImport();
      if (n === null) return;
      await this.refreshPersonas();
      this.showToast(`Added ${n} persona${n === 1 ? "" : "s"}.`);
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  /** The lab pack switch. Seats that named a lab seat fall back to Generic. */
  async setLabPack(on: boolean) {
    await this.saveSettings({ show_lab_pack: on });
    await this.refreshPersonas();
    this.render();
  }

  // ---- Settings ----

  async openSettings() {
    this.set({ view: "settings" });
    await Promise.all([this.refreshKeys(), this.refreshClaude(), this.refreshClis(), this.refreshPersonas()]);
    this.render();
  }

  async saveSettings(patch: Partial<State["settings"]>) {
    const settings = { ...this.state.settings, ...patch };
    this.state.settings = settings;
    this.render();
    try {
      await this.api.settingsSet(settings);
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async setTerminal(on: boolean) {
    try {
      const msg = await this.api.terminalCommand(on);
      await this.saveSettings({ terminal_command: on });
      this.showToast(msg);
    } catch (e) {
      this.showToast(errorText(e));
      this.render();
    }
  }

  async copyDiagnostics() {
    try {
      await this.api.copyText(await this.api.diagnostics());
      this.showToast("Copied. Paste it into your message.");
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async checkUpdate(manual: boolean) {
    try {
      const u = await this.api.checkUpdate();
      if (u) {
        this.updateInstall = u.install;
        this.set({ update: { version: u.version } });
      } else if (manual) this.showToast("You have the latest version.");
    } catch {
      if (manual) this.showToast("Could not check for updates. Try again later.");
    }
  }

  async installUpdate() {
    if (!this.updateInstall) return;
    this.showToast("Installing the update...");
    try {
      await this.updateInstall();
      await this.api.restart();
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  connectedCount(): number {
    return connectedHeads(this.state.keys, this.cliHeads()).length;
  }
}
