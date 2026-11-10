// The main screen: past roundtables on the left, the ask box or results in the centre.

import type { App } from "../app";
import { button, h, toggle } from "../dom";
import { renderMarkdown } from "../markdown";
import { providerName } from "../providers";
import { allFailed, estimateLine, filterHistory, firstFailure, firstLine, shortDate, splitVerdict, statusDot, tableHeads, type RunView } from "../state";
import { answerCard, logo } from "./onboarding";

export function renderMain(app: App): HTMLElement {
  return h("div", { class: "shell" }, sidebar(app), h("main", { class: "centre" }, topbar(app), centre(app)));
}

function topbar(app: App): HTMLElement {
  const dot = statusDot(app.state);
  const n = tableHeads(app.state.settings, app.state.keys).length;
  return h(
    "div",
    { class: "topbar" },
    h("div", { class: "brand small" }, logo(), h("span", { text: "Roundtable" })),
    h(
      "div",
      { class: "top-right" },
      h("span", {
        class: `status-dot ${dot}`,
        title: dot === "ok" ? `Ready. ${n} AI${n === 1 ? "" : "s"} at the table.` : "Not ready. Open Settings.",
        attrs: { role: "img", "aria-label": dot === "ok" ? "Ready" : "Not ready" },
      }),
      h("button", { class: "icon-btn", type: "button", title: "Settings", attrs: { "aria-label": "Settings" }, text: "⚙", on: { click: () => void app.openSettings() } }),
    ),
  );
}

function sidebar(app: App): HTMLElement {
  const s = app.state;
  const list = filterHistory(s.history, s.historyFilter);
  let menu: HTMLElement | null = null;
  const closeMenu = () => {
    menu?.remove();
    menu = null;
  };
  return h(
    "aside",
    { class: "sidebar" },
    h("div", { class: "side-head" }, h("h2", { text: "Past roundtables" }), button("New", () => app.newQuestion(), "btn small")),
    h("input", {
      class: "search",
      type: "search",
      placeholder: "Search",
      value: s.historyFilter,
      attrs: { "aria-label": "Search past roundtables" },
      on: {
        input: (e) => {
          s.historyFilter = (e.target as HTMLInputElement).value;
          const pos = (e.target as HTMLInputElement).selectionStart;
          app.render();
          const el = document.querySelector<HTMLInputElement>(".search");
          el?.focus();
          if (pos !== null) el?.setSelectionRange(pos, pos);
        },
      },
    }),
    !s.settings.history ? h("p", { class: "small muted pad", text: "History is off. Turn it on in Settings." }) : null,
    list.length === 0 && s.settings.history ? h("p", { class: "small muted pad", text: s.history.length ? "Nothing matches." : "Your roundtables will show up here." }) : null,
    h(
      "ul",
      { class: "history" },
      list.map((e) =>
        h(
          "li",
          {
            class: s.opened?.id === e.id ? "active" : "",
            attrs: { tabindex: "0", "data-id": e.id },
            on: {
              click: () => void app.openHistory(e.id),
              keydown: (ev) => {
                if (ev.key === "Enter") void app.openHistory(e.id);
              },
              contextmenu: (ev) => {
                ev.preventDefault();
                closeMenu();
                menu = h(
                  "div",
                  { class: "menu", role: "menu" },
                  h("button", {
                    type: "button",
                    class: "menu-item danger",
                    text: "Delete",
                    attrs: { role: "menuitem" },
                    on: { click: () => { closeMenu(); void app.deleteHistory(e.id); } },
                  }),
                );
                menu.style.left = `${ev.clientX}px`;
                menu.style.top = `${ev.clientY}px`;
                document.body.appendChild(menu);
                setTimeout(() => document.addEventListener("click", closeMenu, { once: true }), 0);
              },
            },
          },
          h("span", { class: "q", text: firstLine(e.question, 60) }),
          h("span", { class: "meta" }, e.source === "claude" ? h("span", { class: "badge", text: "Claude" }) : null, h("span", { text: shortDate(e.ts) })),
        ),
      ),
    ),
  );
}

function centre(app: App): HTMLElement {
  const s = app.state;
  if (s.run) return results(app, s.run);
  if (s.opened) {
    return h(
      "div",
      { class: "pane" },
      h("div", { class: "actions left" }, button("New question", () => app.newQuestion(), "btn"), button("Ask a follow-up", () => app.followUp(), "btn"), button("Copy all", () => void app.api.copyText(s.opened!.transcript).then(() => app.showToast("Copied.")), "btn ghost"), button("Save as file", () => void app.api.saveTextFile("roundtable.md", s.opened!.transcript), "btn ghost")),
      h("div", { class: "transcript card" }, renderMarkdown(s.opened.transcript)),
    );
  }
  return askBox(app);
}

function askBox(app: App): HTMLElement {
  const s = app.state;
  const ask = s.ask;
  const n = tableHeads(s.settings, s.keys).length;
  const est = h("p", { class: "estimate small muted", text: estimateLine(n, ask.mode) });
  const go = button("Ask the table", () => void app.ask(ask.question, ask.mode, ask.research, { path: ask.contextPath, text: ask.contextText }), "btn primary large", n === 0 || !ask.question.trim());
  const choice = (mode: "quick" | "talk", title: string, sub: string) =>
    h(
      "button",
      {
        type: "button",
        class: `choice${ask.mode === mode ? " on" : ""}`,
        attrs: { "aria-pressed": String(ask.mode === mode), "data-mode": mode },
        on: { click: () => app.set({ ask: { ...ask, mode } }) },
      },
      h("strong", { text: title }),
      h("span", { text: sub }),
    );
  return h(
    "div",
    { class: "pane ask" },
    h("textarea", {
      class: "question big",
      rows: 4,
      placeholder: "What do you want to decide?",
      value: ask.question,
      attrs: { "aria-label": "Your question" },
      on: {
        input: (e) => {
          ask.question = (e.target as HTMLTextAreaElement).value;
          go.disabled = n === 0 || !ask.question.trim();
        },
        keydown: (e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && !go.disabled) go.click();
        },
      },
    }),
    h(
      "div",
      { class: "choices" },
      choice("quick", "Quick answers", "Each AI answers once, on its own."),
      choice("talk", "Talk it through", "They read each other and revise. Claude, as chair, says when they agree."),
    ),
    h(
      "div",
      { class: "options" },
      toggle("Let them search the web", ask.research, (v) => app.set({ ask: { ...ask, research: v } })),
      h(
        "div",
        { class: "file-row" },
        ask.contextName
          ? h("span", { class: "file-chip" }, h("span", { text: `Background: ${ask.contextName}` }), h("button", { type: "button", class: "chip-x", text: "×", attrs: { "aria-label": "Remove file" }, on: { click: () => app.set({ ask: { ...ask, contextName: null, contextPath: null, contextText: null } }) } }))
          : button("Add a file", async () => {
              const p = await app.api.pickTextFile();
              if (p) app.set({ ask: { ...ask, contextPath: p, contextText: null, contextName: p.split(/[\\/]/).pop() ?? p } });
            }, "btn small ghost"),
        h("small", { class: "muted", text: "Optional. A text file the AIs should read first." }),
      ),
    ),
    est,
    h("div", { class: "actions left" }, go, n === 0 ? button("Open Settings", () => void app.openSettings(), "btn") : null),
    ask.mode === "talk" && s.keys.claude.status !== "connected"
      ? h("p", { class: "small muted", text: "Talk it through works best with Claude connected, because Claude is the chair." })
      : null,
  );
}

function results(app: App, run: RunView): HTMLElement {
  const last = run.verdicts[run.verdicts.length - 1];
  const verdict =
    run.mode === "talk" && last
      ? (() => {
          const v = splitVerdict(last.text);
          return h(
            "section",
            { class: `verdict ${last.consensus ? "agree" : "disagree"}` },
            h("h2", { text: last.consensus ? "They agree" : run.done ? "They still disagree" : `Round ${last.round}: not yet agreed` }),
            renderMarkdown(v.body),
          );
        })()
      : null;
  const progress = !run.done
    ? h("p", { class: "muted", role: "status", text: run.mode === "talk" ? `Round ${run.round} of up to ${run.rounds}...` : "Asking the table..." })
    : null;
  return h(
    "div",
    { class: "pane results" },
    h("h2", { class: "q-title", text: run.question }),
    h(
      "div",
      { class: "actions left" },
      run.done
        ? [
            button("Copy all", () => void app.api.copyText(run.transcript).then(() => app.showToast("Copied.")), "btn"),
            button("Save as file", () => void app.api.saveTextFile("roundtable.md", run.transcript), "btn"),
            button("Ask a follow-up", () => app.followUp(), "btn"),
            button("New question", () => app.newQuestion(), "btn ghost"),
          ]
        : button("Stop", () => void app.stop(), "btn danger"),
    ),
    progress,
    run.stopped ? h("p", { class: "status bad", text: "Stopped." }) : null,
    run.done && !run.stopped && allFailed(run) ? h("p", { class: "status bad", role: "alert", text: firstFailure(run) }) : null,
    verdict,
    h("div", { class: "answers grid" }, run.heads.map((hd) => answerCard(run.cards[hd] ?? { head: hd, status: "waiting", text: "", round: 1 }))),
    h("p", { class: "small muted", text: `At the table: ${run.heads.map(providerName).join(", ")}.` }),
  );
}
