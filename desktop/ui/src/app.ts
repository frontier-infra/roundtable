// The controller: holds state, runs actions against the backend, re-renders.

import { errorText, type Api, type EngineEvent, type HeadId } from "./api";
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
  tableHeads,
  TALK_ROUNDS,
  type State,
  type Step,
} from "./state";
import { renderOnboarding } from "./views/onboarding";
import { renderMain } from "./views/main";
import { renderSettings } from "./views/settings";
import { h } from "./dom";
import { HEAD_ORDER } from "./providers";

const UPDATE_EVERY_MS = 6 * 60 * 60 * 1000;

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
    if (this.state.view !== "onboarding") await this.refreshHistory();
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
    } catch (e) {
      this.state.clis = { ...before, loaded: true, loading: false, error: errorText(e) };
    }
    this.render();
  }

  async setCli(head: string, on: boolean) {
    const set = new Set(this.state.settings.cli_heads ?? []);
    if (on) set.add(head);
    else set.delete(head);
    await this.saveSettings({ cli_heads: HEAD_ORDER.filter((h) => set.has(h)), clis_chosen: true });
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

  /** Start a roundtable. Used by screen 5 and the main screen. */
  async ask(question: string, mode: "quick" | "talk", research: boolean, context?: { path?: string | null; text?: string | null }) {
    const heads = tableHeads(this.state.settings, this.state.keys, this.cliHeads());
    if (!question.trim() || !heads.length) return;
    this.state.run = newRun(question.trim(), heads, mode);
    this.state.opened = null;
    this.render();
    const onEvent = (e: EngineEvent) => {
      if (!this.state.run || this.state.run.stopped) return;
      this.state.run = applyEvent(this.state.run, e);
      this.render();
      if (e.event === "run_done") void this.refreshHistory().then(() => this.render());
    };
    try {
      await this.api.askStart(
        { question: question.trim(), heads, rounds: mode === "quick" ? 1 : TALK_ROUNDS, research, contextPath: context?.path ?? null, contextText: context?.text ?? null },
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
    this.state.ask = { ...this.state.ask, question: "", contextPath: null, contextText: null, contextName: null };
    this.render();
  }

  followUp() {
    const r = this.state.run;
    const transcript = r?.transcript || this.state.opened?.transcript || "";
    this.state.run = null;
    this.state.opened = null;
    this.state.ask = { ...this.state.ask, question: "", contextPath: null, contextText: transcript, contextName: "The last roundtable" };
    this.render();
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
    await this.refreshHistory();
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

  // ---- Settings ----

  async openSettings() {
    this.set({ view: "settings" });
    await Promise.all([this.refreshKeys(), this.refreshClaude(), this.refreshClis()]);
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
