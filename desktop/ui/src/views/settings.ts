// Settings (spec section 5, "Settings").

import type { App } from "../app";
import { button, h, toggle } from "../dom";
import { CHAIR_DEFAULT_MODEL, HELP_URL, ISSUES_URL, PROVIDERS, providerName } from "../providers";
import { connectedHeads, helpUrl, keychainName, roundtables } from "../state";
import { claudeConnect, cliList, keyCard } from "./onboarding";
import { select, withGlyph } from "./main";
import { VERDICT_SHAPES } from "../session";
import { lockup } from "../brand";
import { personasSection } from "./personas";
import { providersSection } from "./providers";
import { decisionSection } from "../decision";
import { aboutSection } from "../credits";

function section(title: string, ...body: (HTMLElement | null)[]): HTMLElement {
  return h("section", { class: "setting" }, h("h2", { text: title }), h("div", { class: "setting-body" }, body));
}

export function renderSettings(app: App): HTMLElement {
  const s = app.state;
  const connected = connectedHeads(s.keys, app.cliHeads());
  const seated = new Set(s.settings.default_heads.length ? s.settings.default_heads : connected);

  const seats = connected.map((id) =>
    toggle(providerName(id), seated.has(id), (on) => {
      const next = new Set(seated);
      if (on) next.add(id);
      else next.delete(id);
      if (next.size === 0) {
        app.showToast("Keep at least one AI at the table.");
        app.render();
        return;
      }
      void app.saveSettings({ default_heads: connected.filter((id) => next.has(id)) });
    }),
  );

  const models = h(
    "div",
    { class: "grid2" },
    [...PROVIDERS.map((p) => ({ id: p.id, label: p.name, def: p.defaultModel })), { id: "chair", label: "Chair (Claude)", def: CHAIR_DEFAULT_MODEL }].map((m) =>
      h(
        "label",
        { class: "field" },
        h("span", { text: m.label }),
        h("input", {
          type: "text",
          value: s.settings.models[m.id] ?? "",
          placeholder: m.def,
          attrs: { spellcheck: "false" },
          on: {
            change: (e) => {
              const v = (e.target as HTMLInputElement).value.trim();
              const models = { ...s.settings.models };
              if (v) models[m.id] = v;
              else delete models[m.id];
              void app.saveSettings({ models });
            },
          },
        }),
      ),
    ),
  );

  return h(
    "div",
    { class: "settings-page" },
    h("div", { class: "topbar" }, withGlyph(button("Back", () => app.set({ view: "main" }), "btn ghost"), "back"), h("h1", { text: "Settings" }), lockup()),
    h(
      "div",
      { class: "settings" },
      section(
        "AI accounts",
        h("h3", { text: "Apps on this computer" }),
        h("p", { class: "small muted", text: "Tick an app to use it with your own plan. Nothing is sent until you ask a question." }),
        h("div", { class: "stack" }, cliList(app, true)),
        h("h3", { text: "API keys" }),
        h("p", { class: "small muted", text: `Keys are saved in ${keychainName(s.os)}. Roundtable never shows a saved key again.` }),
        h("div", { class: "cards" }, PROVIDERS.map((p) => keyCard(app, p, { removable: true }))),
        ...providersSection(app),
      ),
      section(
        "Who sits at the table",
        seats.length ? h("div", { class: "stack" }, seats) : h("p", { class: "muted", text: "Connect an AI above first." }),
      ),
      section(
        "How the table works",
        toggle("Start with the question coach", s.settings.coach !== false, (v) => {
          app.state.ask = { ...app.state.ask, coach: v };
          void app.saveSettings({ coach: v });
        }, "One AI asks up to three questions and writes a brief. You can edit or skip it."),
        toggle("Blind first round", s.settings.blind_first_round !== false, (v) => void app.saveSettings({ blind_first_round: v }), "In round 1 each seat answers without seeing the others. Off: each seat reads the seats before it."),
        h(
          "label",
          { class: "field inline" },
          h("span", { text: "Verdict to start with" }),
          select("Verdict to start with", s.settings.verdict_shape || "decision", VERDICT_SHAPES.map((v) => ({ value: v.id, label: v.label })), (v) => {
            app.state.ask = { ...app.state.ask, verdict: v as typeof s.settings.verdict_shape };
            void app.saveSettings({ verdict_shape: v as typeof s.settings.verdict_shape });
          }),
        ),
      ),
      section("Personas", ...personasSection(app)),
      decisionSection(app),
      section("Claude connection", h("div", { class: "stack" }, claudeConnect(app, true))),
      section(
        "History",
        toggle("Keep past roundtables on this computer", s.settings.history, (v) => void app.saveSettings({ history: v }), "Nothing leaves this computer."),
        toggle("Add a Roundtable credit line to exports", s.settings.export_credit !== false, (v) => void app.saveSettings({ export_credit: v }), "One line at the end of a saved or copied Markdown file."),
        clearAllRow(app),
      ),
      section(
        "Updates",
        h("p", { text: `Version ${s.version || "unknown"}` }),
        toggle("Install updates automatically", s.settings.auto_update, (v) => void app.saveSettings({ auto_update: v })),
        h("div", { class: "actions left" }, button("Check now", () => void app.checkUpdate(true), "btn")),
      ),
      h(
        "details",
        { class: "setting advanced" },
        h("summary", null, h("h2", { text: "Advanced" })),
        h(
          "div",
          { class: "setting-body" },
        h("h3", { text: "Model per AI" }),
        h("p", { class: "small muted", text: "Leave a box empty to use the default shown." }),
        models,
        h("h3", { text: "Time limit per AI" }),
        h(
          "label",
          { class: "field inline" },
          h("span", { text: "Seconds" }),
          h("input", {
            type: "number",
            value: s.settings.timeout_secs ? String(s.settings.timeout_secs) : "",
            placeholder: "300 (360 with web search)",
            attrs: { min: "30", max: "1800" },
            on: {
              change: (e) => {
                const n = parseInt((e.target as HTMLInputElement).value, 10);
                void app.saveSettings({ timeout_secs: Number.isFinite(n) && n >= 30 ? Math.min(n, 1800) : null });
              },
            },
          }),
        ),
        h("h3", { text: "Terminal" }),
        toggle("Use Roundtable in Terminal too", s.settings.terminal_command, (v) => void app.setTerminal(v), "Adds a roundtable command."),
        s.importable.length
          ? h(
              "div",
              { class: "notice" },
              h("p", { text: `The command-line version has ${s.importable.length} key${s.importable.length === 1 ? "" : "s"} in a plain text file. Move ${s.importable.length === 1 ? "it" : "them"} to ${keychainName(s.os)}?` }),
              button("Import keys from the command-line version", () => {
                const remove = confirm("Keys are now in the keychain. Also remove them from the old text file?");
                void app.importKeys(remove);
              }, "btn small"),
            )
          : null,
        ),
      ),
      section("About", ...aboutSection(app)),
      section(
        "Help",
        h(
          "div",
          { class: "actions left" },
          button("Help pages", () => void app.api.openUrl(HELP_URL), "btn"),
          button("Report a problem", () => void app.api.openUrl(helpUrl(ISSUES_URL, undefined, s.os, s.version)), "btn"),
          button("Copy diagnostics", () => void app.copyDiagnostics(), "btn ghost"),
        ),
        h("p", { class: "small muted", text: "Diagnostics list versions, your system and which AIs are set up. Never keys." }),
      ),
    ),
  );
}

/** Settings, History: delete every roundtable, after one plain confirm. No Undo. */
function clearAllRow(app: App): HTMLElement {
  const n = app.state.history.length;
  const b = button("Clear all history", () => {
    if (confirm(clearAllQuestion(n))) void app.clearHistory();
  }, "btn ghost danger", n === 0);
  b.setAttribute("aria-label", n ? `Clear all history: delete ${roundtables(n)}` : "Clear all history (nothing to delete)");
  return h("div", { class: "actions left" }, b, h("span", { class: "small muted", text: n ? `${roundtables(n)} on this computer.` : "No roundtables on this computer." }));
}

export function clearAllQuestion(n: number): string {
  return `Delete all ${roundtables(n)}? This cannot be undone.`;
}
