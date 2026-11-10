// The five first-run screens (spec section 4).

import type { App } from "../app";
import type { CliDetection, HeadId } from "../api";
import { button, h } from "../dom";
import { PROVIDERS, CLAUDE_DOWNLOAD_URL, ISSUES_URL, type Provider } from "../providers";
import {
  allFailed,
  canContinue,
  connectedHeads,
  cliCardText,
  canSkip,
  cardStatusText,
  firstFailure,
  helpUrl,
  looksLikeKey,
  setupFailedMessage,
  STEPS,
  type HeadCard,
} from "../state";
import { renderMarkdown } from "../markdown";
import { glyph, headLogo, lockup, mark, tableScene, WELCOME_SEATS } from "../brand";
import { providerName } from "../providers";

const STEP_NAMES = ["Welcome", "Set up", "Your AIs", "Claude", "First question"];

export function renderOnboarding(app: App): HTMLElement {
  const s = app.state;
  const screen =
    s.step === 1 ? welcome(app) : s.step === 2 ? setup(app) : s.step === 3 ? keys(app) : s.step === 4 ? claude(app) : firstRun(app);
  return h(
    "main",
    { class: "onboarding", attrs: { "data-step": String(s.step) } },
    rail(app),
    h("div", { class: "stage" }, h("section", { class: "screen" }, screen), footer(app)),
  );
}

/** The left rail: the lockup, the table as it fills, and the five steps. */
function rail(app: App): HTMLElement {
  const s = app.state;
  const seated = s.step === 1 ? WELCOME_SEATS : connectedHeads(s.keys, app.cliHeads());
  const label =
    s.step === 1
      ? "A round table with six AIs: Claude, ChatGPT, Gemini, Grok, GLM and MiniMax."
      : seated.length
        ? `At the table: ${seated.map(providerName).join(", ")}.`
        : "An empty round table with six seats.";
  return h(
    "aside",
    { class: "rail" },
    lockup(),
    tableScene(seated, label),
    h(
      "ol",
      { class: "steps", attrs: { "aria-label": "Setup steps" } },
      STEP_NAMES.map((name, i) => {
        const n = i + 1;
        const state = n < s.step ? "done" : n === s.step ? "current" : "";
        return h(
          "li",
          { class: state, attrs: n === s.step ? { "aria-current": "step" } : {} },
          h("span", { class: "step-mark", attrs: { "aria-hidden": "true" } }, n < s.step ? glyph("check") : String(n)),
          h("span", { text: name }),
        );
      }),
    ),
  );
}

/** Kept for the main screen and settings, which show the lockup in their top bars. */
export function logo(): HTMLElement {
  return mark();
}

function footer(app: App): HTMLElement {
  const s = app.state;
  if (s.step === 1) return h("footer", { class: "nav" });
  const right: HTMLElement[] = [];
  if (canSkip(s.step) && s.step < 5) right.push(button("Skip for now", () => app.goNext(), "btn ghost"));
  if (s.step === 5) right.push(button("Skip for now", () => void app.finishOnboarding(), "btn ghost"));
  if (s.step < 5) right.push(button("Continue", () => app.goNext(), "btn primary", !canContinue({ ...s, clis: s.clis, settings: s.settings })));
  return h(
    "footer",
    { class: "nav" },
    button("Back", () => app.goBack(), "btn ghost", !!(s.run && !s.run.done)),
    h("span", { class: "step-label", attrs: { "aria-live": "polite" }, text: `Step ${s.step} of ${STEPS}` }),
    h("div", { class: "nav-right" }, right),
  );
}

// ---- Screen 1 ----

function welcome(app: App): HTMLElement[] {
  return [
    h("h1", { text: "Ask several AIs at once. Get one clear answer." }),
    h(
      "ul",
      { class: "plain big" },
      h("li", { text: "Roundtable asks Claude, ChatGPT, Gemini and others the same question." }),
      h("li", { text: "You see each answer, side by side." }),
      h("li", { text: "Everything runs on this computer, with your own accounts." }),
    ),
    h("p", { class: "small muted", text: "Each AI company charges your account for the answers. Roundtable itself is free." }),
    h("div", { class: "actions" }, button("Get started", () => app.goNext(), "btn primary large")),
  ];
}

// ---- Screen 2 ----

const TICKS = ["Putting Roundtable in place", "Checking it runs", "Ready"];

function setup(app: App): HTMLElement[] {
  const st = app.state.setup;
  const out: HTMLElement[] = [
    h("h1", { text: "Set up Roundtable" }),
    h("p", { class: "lead", text: "This takes a few seconds. You do not need to install anything else." }),
  ];
  if (st.status !== "idle") {
    out.push(
      h(
        "ol",
        { class: "ticks" },
        TICKS.map((t, i) => {
          const done = i < st.ticks;
          const failed = st.status === "failed" && i === st.ticks;
          const working = st.status === "working" && i === st.ticks;
          return h(
            "li",
            { class: done ? "done" : failed ? "failed" : working ? "working" : "" },
            h("span", { class: "tick", attrs: { "aria-hidden": "true" }, text: done ? "✓" : failed ? "!" : "" }),
            h("span", { text: t }),
          );
        }),
      ),
    );
  }
  if (st.status === "idle") out.push(h("div", { class: "actions" }, button("Set up", () => void app.runSetup(), "btn primary large")));
  if (st.status === "working") out.push(h("div", { class: "actions" }, button("Setting up...", () => {}, "btn primary large", true)));
  if (st.status === "done") out.push(h("p", { class: "ok-line", text: "All set. Click Continue." }));
  if (st.status === "failed") {
    out.push(
      h("p", { class: "error-line", role: "alert", text: setupFailedMessage(st.attempts) }),
      h(
        "div",
        { class: "actions" },
        button("Try again", () => void app.runSetup(), "btn primary"),
        button("Get help", () => void app.api.openUrl(helpUrl(ISSUES_URL, st.error?.code, app.state.os, app.state.version)), "btn"),
      ),
      h("p", { class: "small muted", text: `Error code: ${st.error?.code ?? "E_UNKNOWN"}` }),
    );
  }
  return out;
}

// ---- Screen 3 ----

export function keyCard(app: App, p: Provider, opts: { removable?: boolean } = {}): HTMLElement {
  const card = app.state.keys[p.id];
  let draft = "";
  const input = h("input", {
    type: "password",
    class: "key-input",
    placeholder: "Paste your key here",
    attrs: { "aria-label": `${p.name} key`, autocomplete: "off", spellcheck: "false" },
    disabled: card.status === "checking",
    on: {
      input: (e) => {
        draft = (e.target as HTMLInputElement).value;
        save.disabled = !looksLikeKey(draft);
      },
      // Check at once when a key is pasted.
      paste: () =>
        setTimeout(() => {
          draft = input.value;
          if (looksLikeKey(draft)) void app.saveKey(p.id, draft);
        }, 0),
      keydown: (e) => {
        if (e.key === "Enter" && looksLikeKey(draft)) void app.saveKey(p.id, draft);
      },
    },
  });
  const save = button("Check and save", () => void app.saveKey(p.id, draft), "btn small", true);
  const connected = card.status === "connected";
  const status =
    card.status === "checking"
      ? h("p", { class: "status checking", role: "status", text: "Checking your key..." })
      : card.status === "connected"
        ? h("p", { class: "status ok", role: "status", text: card.message ?? "Connected." })
        : card.status === "error"
          ? h("p", { class: "status bad", role: "alert", text: card.message ?? "That key did not work." })
          : null;
  return h(
    "div",
    { class: `ai-card${connected ? " connected" : ""}`, attrs: { "data-head": p.id } },
    h(
      "div",
      { class: "ai-head" },
      headLogo(p.id),
      h("div", { class: "ai-name" }, h("strong", { text: p.name }), h("span", { class: "muted", text: ` by ${p.company}` })),
    ),
    p.tag ? h("p", { class: "tag", text: p.tag }) : null,
    connected
      ? h(
          "div",
          { class: "row" },
          button("Replace key", () => {
            app.state.keys = { ...app.state.keys, [p.id]: { status: "empty" } };
            app.render();
          }, "btn small"),
          opts.removable ? button("Remove", () => void app.removeKey(p.id), "btn small ghost") : null,
        )
      : h("div", { class: "row" }, input, save),
    status,
    connected
      ? null
      : h(
          "p",
          { class: "how small muted" },
          h("a", {
            href: "#",
            text: "Where do I get a key?",
            on: {
              click: (e) => {
                e.preventDefault();
                void app.api.openUrl(p.keyPage);
              },
            },
          }),
          " Sign in. Click Create key. Copy it and paste it here.",
        ),
  );
}

/** One card per AI tool found on this computer. */
export function cliCard(app: App, d: CliDetection): HTMLElement {
  const s = app.state;
  const on = app.cliHeads().includes(d.head);
  const canTick = d.signed_in !== false;
  const signing = s.clis.signingIn.includes(d.id);
  const id = `cli-${d.id}`;
  const line = signing && d.signed_in === false ? "Finish signing in in the new window, then click Check again." : cliCardText(d, s.os);
  return h(
    "div",
    { class: `cli-card${on ? " on" : ""}${d.signed_in === false ? " signed-out" : ""}`, attrs: { "data-cli": d.id } },
    h(
      "label",
      { class: "cli-main", for: id },
      h("input", {
        id,
        type: "checkbox",
        class: "cli-check",
        checked: on,
        disabled: !canTick,
        attrs: { "aria-label": `Use ${d.name}` },
        on: { change: (e) => void app.setCli(d.head, (e.target as HTMLInputElement).checked) },
      }),
      headLogo(d.head),
      h(
        "span",
        { class: "cli-text" },
        h("span", { class: "cli-name" }, h("strong", { text: d.name }), d.version ? h("span", { class: "cli-version", text: d.version, title: d.how }) : null),
        h("span", { class: "cli-line", text: line }),
      ),
    ),
    d.signed_in === false
      ? h(
          "div",
          { class: "cli-actions" },
          signing
            ? button(s.clis.loading ? "Checking..." : "Check again", () => void app.refreshClis(), "btn small", s.clis.loading)
            : button("Sign in", () => void app.cliSignIn(d.id), "btn small primary"),
        )
      : null,
  );
}

/** The tools found on this computer, with a "Look again" button. */
export function cliList(app: App, inSettings = false): HTMLElement[] {
  const c = app.state.clis;
  const found = c.list.filter((d) => d.found);
  const out: HTMLElement[] = [];
  if (!c.loaded) {
    out.push(h("p", { class: "muted", role: "status", text: "Looking for AI apps on this computer..." }));
    return out;
  }
  if (found.length) out.push(h("div", { class: "cards" }, found.map((d) => cliCard(app, d))));
  else if (inSettings) out.push(h("p", { class: "muted", text: "No AI apps found on this computer." }));
  if (c.error) out.push(h("p", { class: "status bad", role: "alert", text: c.error }));
  out.push(h("div", { class: "actions left" }, button(c.loading ? "Looking..." : "Look again", () => void app.refreshClis(), "btn small ghost", c.loading)));
  return out;
}

function keyCards(app: App, found: number): HTMLElement[] {
  const s = app.state;
  const primary = PROVIDERS.filter((p) => p.primary);
  const more = PROVIDERS.filter((p) => !p.primary);
  const moreConnected = more.filter((p) => s.keys[p.id].status === "connected").length;
  const out: (HTMLElement | null)[] = [
    h("div", { class: "cards" }, primary.map((p) => keyCard(app, p))),
    h(
      "button",
      {
        class: "more-toggle",
        type: "button",
        attrs: { "aria-expanded": String(s.moreOpen) },
        on: { click: () => app.set({ moreOpen: !s.moreOpen }) },
      },
      glyph("chevron", "glyph chev"),
      h("span", { text: `More AIs${moreConnected ? ` (${moreConnected} connected)` : ""}` }),
    ),
    s.moreOpen ? h("div", { class: "cards" }, more.map((p) => keyCard(app, p))) : null,
    h("p", {
      class: "small muted plan-note",
      text: found
        ? "A chat plan like Claude Pro or ChatGPT Plus is not an API key. Your plan already works through the apps above."
        : "A chat plan like Claude Pro or ChatGPT Plus is not an API key. Use the link on a card to get a key.",
    }),
  ];
  return out.filter((x): x is HTMLElement => !!x);
}

function keys(app: App): HTMLElement[] {
  const s = app.state;
  const c = s.clis;
  const found = c.list.filter((d) => d.found).length;
  const keysOpen = s.keysOpen ?? (c.loaded && found === 0);
  const keyed = PROVIDERS.filter((p) => s.keys[p.id].status === "connected").length;
  const title = !c.loaded ? "Connect your AIs" : found ? "We found these on your computer" : "Connect at least one AI";
  const lead = !c.loaded
    ? "Looking for the AI apps you already use."
    : found
      ? "Roundtable can use them with the plans you already have. Two or more make a real roundtable."
      : "We did not find Claude Code, Codex or Gemini CLI on this computer. Paste an API key instead.";
  return [
    h("h1", { text: title }),
    h("p", { class: "lead", text: lead }),
    ...(found || !c.loaded ? cliList(app) : []),
    h(
      "button",
      {
        class: "more-toggle keys-toggle",
        type: "button",
        attrs: { "aria-expanded": String(keysOpen) },
        on: { click: () => app.set({ keysOpen: !keysOpen }) },
      },
      glyph("chevron", "glyph chev"),
      h("span", { text: `Or use an API key${keyed ? ` (${keyed} connected)` : ""}` }),
    ),
    ...(keysOpen ? [h("div", { class: "key-section" }, keyCards(app, found))] : []),
  ];
}

// ---- Screen 4 ----

export function claudeConnect(app: App, inSettings = false): HTMLElement[] {
  const c = app.state.claude;
  if (c.loading) return [h("p", { class: "muted", role: "status", text: "Looking for Claude on this computer..." })];
  const out: HTMLElement[] = [];
  if (c.app_found) {
    out.push(
      h(
        "div",
        { class: `connect-row${c.app_connected ? " done" : ""}`, attrs: { "data-target": "app" } },
        headLogo("claude"),
        h("div", { class: "connect-text" }, h("strong", { text: "Claude app found." }), c.appMessage ? h("p", { class: "status ok", text: c.appMessage }) : c.app_connected ? h("p", { class: "status ok", text: "Connected." }) : null),
        c.app_connected
          ? h("span", { class: "check", attrs: { role: "img", "aria-label": "Connected" } }, glyph("check"))
          : button(c.busy === "app" ? "Connecting..." : "Connect to the Claude app", () => void app.connectClaude("app"), "btn primary", !!c.busy),
      ),
    );
  }
  if (c.code_found) {
    out.push(
      h(
        "div",
        { class: `connect-row${c.code_connected ? " done" : ""}`, attrs: { "data-target": "code" } },
        headLogo("claude"),
        h("div", { class: "connect-text" }, h("strong", { text: "Claude Code found." }), c.codeMessage ? h("p", { class: "status ok", text: c.codeMessage }) : c.code_connected ? h("p", { class: "status ok", text: "Connected." }) : null),
        c.code_connected
          ? h("span", { class: "check", attrs: { role: "img", "aria-label": "Connected" } }, glyph("check"))
          : button(c.busy === "code" ? "Connecting..." : "Connect to Claude Code", () => void app.connectClaude("code"), "btn primary", !!c.busy),
      ),
    );
  }
  if (c.plugin_ids && c.plugin_ids.length) {
    out.push(
      h(
        "div",
        { class: "notice" },
        h("p", { text: "The Roundtable plugin is also installed in Claude Code. Claude would see two Roundtable tools." }),
        button(c.busy === "plugin" ? "Working..." : "Use the app's engine instead", () => void app.connectClaude("plugin"), "btn small", !!c.busy),
      ),
    );
  }
  if (c.pluginMessage) out.push(h("p", { class: "status ok", text: c.pluginMessage }));
  if (!c.app_found && !c.code_found) {
    out.push(
      h(
        "div",
        { class: "notice" },
        h("p", null, "Claude is not on this computer yet. ", h("a", { href: "#", text: "Download Claude", on: { click: (e) => { e.preventDefault(); void app.api.openUrl(CLAUDE_DOWNLOAD_URL); } } })),
      ),
    );
  }
  if (!c.code_found) {
    const lines = (c.plugin_lines ?? ["", ""]).join("\n");
    out.push(
      h("p", { class: "small", text: "Using Claude Code somewhere else? Paste these two lines into Claude Code:" }),
      h(
        "div",
        { class: "copy-box" },
        h("pre", { text: lines }),
        button("Copy", () => void app.api.copyText(lines).then(() => app.showToast("Copied.")), "btn small"),
      ),
    );
  }
  if (c.error) out.push(h("p", { class: "status bad", role: "alert", text: c.error }));
  if (inSettings && (c.app_connected || c.code_connected)) {
    out.push(h("div", { class: "actions left" }, button("Disconnect", () => void app.disconnectClaude(), "btn ghost danger")));
  }
  return out;
}

function claude(app: App): HTMLElement[] {
  return [
    h("h1", { text: "Use Roundtable from inside Claude" }),
    h("p", { class: "lead", text: "Then you can tell Claude “ask the roundtable”, and it will." }),
    h("div", { class: "stack" }, claudeConnect(app)),
  ];
}

// ---- Screen 5 ----

export function answerCard(c: HeadCard): HTMLElement {
  return h(
    "article",
    { class: `answer ${c.status}`, attrs: { "data-head": c.head } },
    h("header", null, headLogo(c.head, "sm"), h("strong", { text: providerName(c.head) }), h("span", { class: `state ${c.status}`, text: cardStatusText(c) })),
    c.text ? renderMarkdown(c.text.replace(/^\s*\**POSITION:\**\s*/i, "")) : c.status === "thinking" || c.status === "waiting" ? h("div", { class: "shimmer", attrs: { "aria-hidden": "true" } }) : h("p", { class: "muted", text: c.message ?? "" }),
  );
}

function firstRun(app: App): HTMLElement[] {
  const s = app.state;
  const run = s.run;
  const out: (HTMLElement | null)[] = [h("h1", { text: "Try it now" })];
  if (!run) {
    let q = s.firstQuestion;
    out.push(
      h("p", { class: "lead", text: "Change the question if you like." }),
      h("textarea", {
        class: "question",
        rows: 3,
        value: q,
        attrs: { "aria-label": "Your question" },
        on: { input: (e) => (q = s.firstQuestion = (e.target as HTMLTextAreaElement).value) },
      }),
      h("div", { class: "actions" }, button("Ask the table", () => void app.ask(q, "quick", false), "btn primary large", app.connectedCount() === 0)),
      app.connectedCount() === 0 ? h("p", { class: "status bad", text: "Connect an AI first. Click Back." }) : null,
    );
    return out.filter((x): x is HTMLElement => !!x);
  }
  out.push(h("p", { class: "lead", text: run.question }));
  out.push(h("div", { class: "answers" }, run.heads.map((hd) => answerCard(run.cards[hd]))));
  if (run.done && allFailed(run)) {
    out.push(
      h("p", { class: "status bad", role: "alert", text: firstFailure(run) }),
      h("div", { class: "actions" }, button("Back to keys", () => app.set({ step: 3, run: null }), "btn primary")),
    );
  } else if (run.done) {
    out.push(
      h("p", { class: "ok-line big", text: "That's it. You're set up." }),
      h("div", { class: "actions" }, button("Open Roundtable", () => void app.finishOnboarding(), "btn primary large")),
    );
  }
  return out.filter((x): x is HTMLElement => !!x);
}

export type { HeadId };
