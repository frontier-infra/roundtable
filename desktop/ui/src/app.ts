// The controller: holds state, runs actions against the backend, re-renders.

import { errorText, type Api, type EngineEvent, type HeadId, type ProviderAddRequest, type VerdictShape } from "./api";
import { attachmentLabel, attachmentsBlock, briefMarkdown, coachText, emptyBrief, looksLikeLink, mdFileName, priorText, runMarkdown, runSeats, sessionMarkdown } from "./session";
import { withCredit } from "./credits";
import {
  applyEvent,
  back,
  cliHeadsOn,
  connectedHeads,
  defaultCliHeads,
  filterHistory,
  flagOnly,
  pickRows,
  roundtables,
  runSource,
  UNDO_MS,
  initialState,
  keyCardFromResult,
  keysFromStatus,
  newRun,
  next,
  planSeats,
  providerSeatsOn,
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
import { HEAD_ORDER, setProviderNames } from "./providers";
import { CHANGELOG, parseChangelog, renderUpdateBanner, renderWhatsNew, whatsNewAfterUpdate } from "./updates";
import { HELP_HOME, helpPage, onMenu, renderAbout, renderHelp } from "./help";

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
  private toastTimer: ReturnType<typeof setTimeout> | null = null;
  /** Deletes still inside their Undo window, by token: the timer that makes them final. */
  private pending = new Map<string, ReturnType<typeof setTimeout>>();
  /** The roundtable to reopen if the last delete is undone. */
  private undoReopen: string | null = null;
  /** A history row to focus after the next render (keyboard use). */
  focusRow: string | null = null;

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
    // Added providers, and Ollama or LM Studio running here. No prompt is sent.
    void this.refreshProviders();
    // Back from a sign-in window: look again.
    if (typeof window !== "undefined") {
      window.addEventListener("focus", () => {
        if (this.state.clis.signingIn.length && !this.state.clis.loading) void this.refreshClis();
      });
    }
    if (typeof window !== "undefined") {
      // Command or Control Z undoes the last delete while Undo is offered.
      window.addEventListener("keydown", (e) => {
        const t = e.target as HTMLElement | null;
        const typing = !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
        if (this.state.undo && !typing && (e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === "z") {
          e.preventDefault();
          void this.undoDelete();
        }
      });
      // Closing the window ends the Undo window. The core finishes any delete left on the next start.
      window.addEventListener("pagehide", () => this.purgePending());
    }
    // The native menu bar (Help, Settings, About...).
    void this.api.onMenu((id) => void onMenu(this, id)).catch(() => {});
    if (opts.view) this.state.view = opts.view;
    else this.state.view = this.state.settings.onboarded && this.state.engineReady ? "main" : "onboarding";
    if (opts.screen) this.state.step = Math.min(5, Math.max(1, opts.screen)) as Step;
    if (this.state.view !== "onboarding") await Promise.all([this.refreshHistory(), this.refreshPersonas()]);
    if (this.state.step === 4) void this.refreshClaude();
    this.noteVersion();
    this.render();
    // Always look, so the person knows a new version is out. The switch decides only
    // whether it downloads and installs by itself.
    void this.checkUpdate(false);
    setInterval(() => void this.checkUpdate(false), UPDATE_EVERY_MS);
  }

  /** After an update, open What's new once. Remember the version either way. */
  private noteVersion() {
    const { version, settings } = this.state;
    if (!version || settings.last_seen_version === version) return;
    const shown = settings.onboarded ? whatsNewAfterUpdate(settings.last_seen_version, version) : null;
    if (shown && this.state.view !== "onboarding") this.state.whatsNew = shown;
    void this.saveSettings({ last_seen_version: version });
  }

  set(patch: Partial<State>) {
    Object.assign(this.state, patch);
    this.render();
  }

  render() {
    const s = this.state;
    const view =
      s.view === "onboarding" ? renderOnboarding(this) : s.view === "settings" ? renderSettings(this) : s.view === "help" ? renderHelp(this) : renderMain(this);
    const banner = renderUpdateBanner(this);
    const sheet = s.whatsNew ? renderWhatsNew(this, s.whatsNew) : s.about ? renderAbout(this) : null;
    const toast = s.toast
      ? h(
          "div",
          { class: s.undo ? "toast has-action" : "toast", role: "status", attrs: { "aria-live": "polite" } },
          h("span", { text: s.toast }),
          s.undo
            ? h("button", { class: "toast-action", type: "button", text: "Undo", attrs: { "aria-label": `Undo delete of ${roundtables(s.undo.count)}` }, on: { click: () => void this.undoDelete() } })
            : null,
        )
      : null;
    this.root.replaceChildren(...[banner, view, sheet, toast].filter((x): x is HTMLElement => !!x));
    if (this.focusRow) {
      const id = this.focusRow;
      [...this.root.querySelectorAll<HTMLElement>(".history li")].find((li) => li.dataset.id === id)?.focus();
      this.focusRow = null;
    }
  }

  /** Open the built-in help on a page. Back returns to the screen it was opened from. */
  openHelp(page = HELP_HOME) {
    const s = this.state;
    const back = s.view === "help" ? s.help.back : s.view;
    const id = helpPage(page) ? page : HELP_HOME;
    const same = s.view === "help" && s.help.page === id;
    this.set({ view: "help", help: { page: id, query: same ? s.help.query : "", back }, about: false, whatsNew: null });
    if (!same) this.root.querySelector(".help-view")?.scrollTo?.({ top: 0 });
  }

  closeHelp() {
    this.set({ view: this.state.help.back, help: { ...this.state.help, query: "" } });
  }

  showToast(msg: string, ms = 3000, undo: State["undo"] = null) {
    this.state.undo = undo;
    this.state.toast = msg;
    this.render();
    if (this.toastTimer) clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => this.set({ toast: null, undo: null }), ms);
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

  /** AIs that sit without a key card: ticked apps, then ticked provider seats (`provider:model`). */
  cliHeads(): string[] {
    return [...cliHeadsOn(this.state), ...providerSeatsOn(this.state)];
  }

  // ---- Providers the person adds ----

  /** The added providers, and Ollama or LM Studio found running (the way apps are found). */
  async refreshProviders() {
    const pv = this.state.providers;
    this.state.providers = { ...pv, loading: true };
    this.render();
    try {
      const [list, local] = await Promise.all([this.api.providersList(), this.api.providersDetect().catch(() => ({ found: [] }))]);
      setProviderNames(list.providers);
      this.state.providers = { ...this.state.providers, loaded: true, loading: false, list: list.providers, presets: list.presets, found: local.found, error: undefined };
      // Found models need no second request.
      const models = { ...this.state.providers.models };
      for (const f of local.found) models[f.preset] ??= { ok: true, provider: f.preset, local: true, total: f.models.length, models: f.models };
      this.state.providers.models = models;
    } catch (e) {
      this.state.providers = { ...this.state.providers, loaded: true, loading: false, error: errorText(e) };
    }
    this.render();
  }

  /** Read one provider's model list. */
  async loadModels(id: string) {
    const pv = this.state.providers;
    this.state.providers = { ...pv, models: { ...pv.models, [id]: { ...(pv.models[id] ?? { ok: true }), loading: true } } };
    this.render();
    let r;
    try {
      r = await this.api.providerModels(id);
    } catch (e) {
      r = { ok: false, message: errorText(e) };
    }
    this.state.providers = { ...this.state.providers, models: { ...this.state.providers.models, [id]: { ...r, loading: false } } };
    this.render();
  }

  /** Add or change a provider, then reload the list and settings (the engine owns them). */
  async saveProvider(req: ProviderAddRequest): Promise<boolean> {
    const id = req.id ?? req.preset;
    this.state.providers = { ...this.state.providers, busy: id };
    this.render();
    let ok = false;
    try {
      const r = await this.api.providerAdd(req);
      ok = r.ok;
      if (!ok) this.showToast(r.message ?? "Could not save the provider.");
    } catch (e) {
      this.showToast(errorText(e));
    }
    this.state.providers = { ...this.state.providers, busy: null };
    if (ok) {
      try {
        this.state.settings = await this.api.settingsGet();
      } catch {
        /* keep */
      }
      await this.refreshProviders();
    } else this.render();
    return ok;
  }

  /** Tick or untick one model. Ticking a model of a found local provider adds that provider. */
  async tickModel(provider: string, model: string, on: boolean) {
    const row = this.state.providers.list.find((p) => p.id === provider);
    const now = row ? row.models.map((m) => m.id) : [];
    const next = on ? [...now.filter((m) => m !== model), model] : now.filter((m) => m !== model);
    const found = !row ? this.state.providers.found.find((f) => f.preset === provider) : undefined;
    await this.saveProvider({ preset: row?.preset ?? found?.preset ?? provider, id: provider, url: found?.base_url, models: next });
    // A newly ticked seat gets one five-word test, so a broken one shows before a run.
    if (on) void this.testHead(`${provider}:${model}`, "auto");
  }

  /** The "Add a provider" form's Connect button: check the address and key, then save. */
  async addProviderFromForm() {
    const f = this.state.providers.form;
    if (!f) return;
    this.state.providers = { ...this.state.providers, form: { ...f, status: "checking", message: undefined } };
    this.render();
    let r;
    try {
      r = await this.api.providerAdd({ preset: f.preset, id: f.id, name: f.name.trim() || undefined, url: f.url.trim() || undefined, key: f.key.trim() || undefined, check: true });
    } catch (e) {
      r = { ok: false, message: errorText(e) };
    }
    if (!r.ok) {
      this.state.providers = { ...this.state.providers, form: { ...f, key: "", status: "error", message: r.message ?? "Could not add that provider." } };
      this.render();
      return;
    }
    const id = r.provider?.id ?? f.preset;
    this.state.providers = { ...this.state.providers, form: null };
    try {
      this.state.settings = await this.api.settingsGet();
    } catch {
      /* keep */
    }
    await this.refreshProviders();
    await this.loadModels(id);
    this.showToast(`${r.provider?.name ?? "Provider"} added. Tick the models you want at the table.`);
  }

  async removeProvider(id: string) {
    try {
      const r = await this.api.providerRemove(id);
      if (r.message) this.showToast(r.message);
    } catch (e) {
      this.showToast(errorText(e));
    }
    const models = { ...this.state.providers.models };
    delete models[id];
    this.state.providers = { ...this.state.providers, models };
    try {
      this.state.settings = await this.api.settingsGet();
    } catch {
      /* keep */
    }
    await this.refreshProviders();
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
    if (flagOnly(question) || !heads.length) return;
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
          source: runSource(this.state.view, question),
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
    if (flagOnly(a.question)) return;
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
      await this.api.copyText(withCredit(runMarkdown(run), this.state.settings.export_credit));
      this.showToast("Copied as Markdown.");
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  async downloadMarkdown(run = this.state.run) {
    if (!run) return;
    try {
      if (await this.api.saveTextFile(mdFileName(run.question), withCredit(runMarkdown(run), this.state.settings.export_credit))) this.showToast("Saved.");
    } catch (e) {
      this.showToast(errorText(e));
    }
  }

  /** Every turn of this session, as one Markdown file. */
  async exportSession() {
    const turns = [...this.state.session, ...(this.state.run ? [this.state.run] : [])];
    if (!turns.length) return;
    try {
      if (await this.api.saveTextFile(mdFileName(turns[0].question, true), withCredit(sessionMarkdown(turns), this.state.settings.export_credit))) this.showToast("Session saved.");
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

  /** Delete roundtables, with Undo for a few seconds. After that the files are gone. */
  async deleteHistory(ids: string[]) {
    ids = [...new Set(ids)];
    if (!ids.length) return;
    const s = this.state;
    // Keyboard focus moves to the row after the last one deleted.
    const order = s.history.map((e) => e.id);
    const last = Math.max(...ids.map((i) => order.indexOf(i)));
    const nextId = order.slice(last + 1).find((i) => !ids.includes(i)) ?? [...order].reverse().find((i) => !ids.includes(i)) ?? null;
    let token: string;
    try {
      token = await this.api.historyDelete(ids);
    } catch (e) {
      this.showToast(errorText(e));
      return;
    }
    this.undoReopen = s.opened && ids.includes(s.opened.id) ? s.opened.id : null;
    if (this.undoReopen) s.opened = null;
    s.historySel = s.historySel.filter((i) => !ids.includes(i));
    if (!s.historySel.length) s.selecting = false;
    if (s.historyAnchor && ids.includes(s.historyAnchor)) s.historyAnchor = null;
    await this.refreshHistory();
    this.pending.set(
      token,
      setTimeout(() => void this.purge(token), UNDO_MS),
    );
    if (document.activeElement?.closest?.(".history") && nextId) this.focusRow = nextId;
    this.showToast(ids.length === 1 ? "Deleted." : `Deleted ${roundtables(ids.length)}.`, UNDO_MS, { token, count: ids.length });
  }

  /** Put the last delete back. */
  async undoDelete() {
    const u = this.state.undo;
    if (!u) return;
    const t = this.pending.get(u.token);
    if (t) clearTimeout(t);
    this.pending.delete(u.token);
    if (this.toastTimer) clearTimeout(this.toastTimer);
    this.state.undo = null;
    this.state.toast = null;
    try {
      await this.api.historyRestore(u.token);
    } catch (e) {
      await this.refreshHistory();
      this.showToast(errorText(e));
      return;
    }
    await this.refreshHistory();
    const reopen = this.undoReopen;
    this.undoReopen = null;
    if (reopen && !this.state.run) {
      await this.openHistory(reopen);
      return;
    }
    this.render();
  }

  private async purge(token: string) {
    this.pending.delete(token);
    if (this.state.undo?.token === token) this.set({ undo: null, toast: null });
    try {
      await this.api.historyPurge(token);
    } catch {
      /* the core finishes it on the next start */
    }
  }

  /** End every Undo window now. */
  purgePending() {
    for (const [token, t] of this.pending) {
      clearTimeout(t);
      void this.purge(token);
    }
  }

  /** A click on a row: plain opens, command or control picks, shift picks a range. */
  clickHistory(id: string, ev: { metaKey?: boolean; ctrlKey?: boolean; shiftKey?: boolean }) {
    const s = this.state;
    const how = ev.shiftKey ? "range" : ev.metaKey || ev.ctrlKey || s.selecting ? "toggle" : null;
    if (!how) {
      s.historySel = [];
      s.historyAnchor = id;
      void this.openHistory(id);
      return;
    }
    // The open roundtable counts as picked when a pick starts from it.
    const base = !s.selecting && !s.historySel.length && s.opened && how === "toggle" && s.opened.id !== id ? [s.opened.id] : s.historySel;
    s.historySel = pickRows(filterHistory(s.history, s.historyFilter), base, s.historyAnchor ?? s.opened?.id ?? null, id, how);
    if (how === "toggle") s.historyAnchor = id;
    s.selecting = s.historySel.length > 0;
    this.focusRow = id;
    this.render();
  }

  /** Select mode on or off. Off clears the picks. */
  setSelecting(on: boolean) {
    this.state.selecting = on;
    if (!on) this.state.historySel = [];
    this.render();
  }

  /** Settings, History: delete every roundtable. The caller confirms first. */
  async clearHistory() {
    for (const t of this.pending.values()) clearTimeout(t);
    this.pending.clear();
    try {
      await this.api.historyClear();
    } catch (e) {
      this.showToast(errorText(e));
      return;
    }
    Object.assign(this.state, { opened: null, historySel: [], selecting: false, historyAnchor: null });
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
    await Promise.all([this.refreshKeys(), this.refreshClaude(), this.refreshClis(), this.refreshPersonas(), this.refreshProviders()]);
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

  // ---- Updates ----

  /** Look for a newer version on the chosen channel. With automatic updates on, download it. */
  async checkUpdate(manual: boolean) {
    if (this.state.updateStatus.checking) return;
    const channel = this.state.settings.update_channel === "test" ? "test" : "stable";
    this.set({ updateStatus: { ...this.state.updateStatus, checking: true } });
    try {
      const u = await this.api.updateCheck(channel);
      const was = this.state.update;
      const update = u
        ? { version: u.version, notes: u.notes, date: u.date, ready: u.ready, busy: was?.version === u.version ? was.busy : null, dismissed: !manual && was?.version === u.version && was.dismissed }
        : null;
      this.set({ update, updateStatus: { checking: false, checkedAt: Date.now(), error: null } });
      if (!u && manual) this.showToast("You have the latest version.");
      if (update && !update.ready && this.state.settings.auto_update) void this.downloadUpdate();
    } catch (e) {
      this.set({ updateStatus: { checking: false, checkedAt: this.state.updateStatus.checkedAt, error: errorText(e) } });
      if (manual) this.showToast("Could not check for updates. Try again later.");
    }
  }

  /** Background download; it installs on the next quit. */
  async downloadUpdate() {
    const u = this.state.update;
    if (!u || u.busy) return;
    this.set({ update: { ...u, busy: "downloading" } });
    try {
      await this.api.updateDownload(true);
      if (this.state.update?.version === u.version) this.set({ update: { ...this.state.update, ready: true, busy: null } });
    } catch (e) {
      if (this.state.update?.version === u.version) this.set({ update: { ...this.state.update, busy: null } });
      this.set({ updateStatus: { ...this.state.updateStatus, error: errorText(e) } });
    }
  }

  /** Update now: install and restart into the new version. */
  async installUpdate() {
    const u = this.state.update;
    if (!u || u.busy === "installing") return;
    this.set({ whatsNew: null, update: { ...u, busy: "installing", dismissed: false } });
    try {
      await this.api.updateInstall();
    } catch (e) {
      this.set({ update: { ...u, busy: null } });
      this.showToast(`The update did not install: ${errorText(e)}`);
    }
  }

  laterUpdate() {
    const u = this.state.update;
    if (!u) return;
    this.set({ update: { ...u, dismissed: true } });
    if (this.state.settings.auto_update) this.showToast("It installs the next time you quit Roundtable.");
  }

  async setAutoUpdate(on: boolean) {
    await this.saveSettings({ auto_update: on });
    const u = this.state.update;
    if (!u) return;
    if (!on) await this.api.updateOnQuit(false).catch(() => {});
    else if (u.ready) await this.api.updateOnQuit(true).catch(() => {});
    else void this.downloadUpdate();
  }

  async setUpdateChannel(channel: "stable" | "test") {
    await this.saveSettings({ update_channel: channel });
    this.set({ update: null });
    await this.checkUpdate(true);
  }

  /** The banner's What's new: the new version's notes from the manifest. */
  showUpdateNotes() {
    const u = this.state.update;
    if (!u) return;
    const fromFile = parseChangelog(CHANGELOG).find((s) => s.version === u.version.replace(/-.*$/, ""));
    const body = u.notes.trim() || fromFile?.body || "No notes for this version.";
    this.set({ whatsNew: { title: `What's new in Roundtable ${u.version}`, sections: [{ version: u.version, date: u.date?.slice(0, 10) ?? "", body }] } });
  }

  /** Settings, What's new: every version, newest first. */
  showChangelog() {
    this.set({ whatsNew: { title: "What's new", sections: parseChangelog(CHANGELOG) } });
  }

  connectedCount(): number {
    return connectedHeads(this.state.keys, this.cliHeads()).length;
  }
}
