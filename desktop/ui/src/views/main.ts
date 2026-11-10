// The main screen: past roundtables on the left, the ask box or results in the centre.

import type { App } from "../app";
import { button, h, toggle } from "../dom";
import { renderMarkdown } from "../markdown";
import { providerName } from "../providers";
import {
  allFailed,
  allPersonas,
  cleanAnswer,
  estimateLine,
  filterHistory,
  findPersona,
  firstFailure,
  firstLine,
  MAX_ROUNDS,
  planSeats,
  roundsHint,
  seatInfos,
  shortDate,
  splitVerdict,
  statusDot,
  tableHeads,
  type RoundStep,
  type RunView,
  type SeatPick,
} from "../state";
import type { PersonaLibrary } from "../api";
import { answerCard } from "./onboarding";
import { glyph, headLogo, lockup } from "../brand";

export function renderMain(app: App): HTMLElement {
  return h("div", { class: "shell" }, sidebar(app), h("main", { class: "centre" }, topbar(app), centre(app)));
}

function topbar(app: App): HTMLElement {
  const dot = statusDot(app.state);
  const n = tableHeads(app.state.settings, app.state.keys, app.cliHeads()).length;
  return h(
    "div",
    { class: "topbar" },
    lockup(),
    h(
      "div",
      { class: "top-right" },
      h(
        "span",
        { class: "readiness", title: dot === "ok" ? `Ready. ${n} AI${n === 1 ? "" : "s"} at the table.` : "Not ready. Open Settings." },
        h("span", {
          class: `status-dot ${dot}`,
          attrs: { role: "img", "aria-label": dot === "ok" ? "Ready" : "Not ready" },
        }),
        h("span", { class: "readiness-text", text: dot === "ok" ? `${n} AI${n === 1 ? "" : "s"} ready` : "Not ready" }),
      ),
      h("button", { class: "icon-btn", type: "button", title: "Settings", attrs: { "aria-label": "Settings" }, on: { click: () => void app.openSettings() } }, glyph("settings")),
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
    h("div", { class: "side-head" }, h("h2", { text: "Past roundtables" }), withGlyph(button("New question", () => app.newQuestion(), "btn small"), "plus")),
    h("label", { class: "search-wrap" }, glyph("search", "glyph search-glyph"), h("input", {
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
    })),
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
  const heads = tableHeads(s.settings, s.keys, app.cliHeads());
  const n = heads.length;
  const plan = planSeats(ask, heads, s.personas);
  const est = h("p", { class: "estimate small muted", text: estimateLine(n, ask.rounds, plan.length) });
  const go = button("Ask the table", () => void app.ask(ask.question, ask.rounds, ask.research, { path: ask.contextPath, text: ask.contextText }, plan), "btn primary large", n === 0 || !ask.question.trim());
  const chairMissing = s.keys.claude.status !== "connected" && !app.cliHeads().includes("claude");
  return h(
    "div",
    { class: "pane ask" },
    h("h1", { class: "ask-title", text: "What do you want to decide?" }),
    h("textarea", {
      class: "question big",
      rows: 4,
      placeholder: "Type your question. Add as much detail as you like.",
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
    h("div", { class: "ask-grid" }, roundsControl(app), seatsControl(app, heads, plan)),
    h(
      "div",
      { class: "options" },
      toggle("Let them search the web", ask.research, (v) => app.set({ ask: { ...ask, research: v } })),
      h(
        "div",
        { class: "file-row" },
        ask.contextName
          ? h("span", { class: "file-chip" }, h("span", { text: `Background: ${ask.contextName}` }), h("button", { type: "button", class: "chip-x", text: "×", attrs: { "aria-label": "Remove file" }, on: { click: () => app.set({ ask: { ...ask, contextName: null, contextPath: null, contextText: null } }) } }))
          : withGlyph(button("Add a file", async () => {
              const p = await app.api.pickTextFile();
              if (p) app.set({ ask: { ...ask, contextPath: p, contextText: null, contextName: p.split(/[\\/]/).pop() ?? p } });
            }, "btn small"), "file"),
        h("small", { class: "muted", text: "Optional. A text file the AIs should read first." }),
      ),
    ),
    h(
      "div",
      { class: "ask-bar" },
      h(
        "div",
        { class: "seat-row", attrs: { "aria-label": n ? `At the table: ${heads.map(providerName).join(", ")}` : "No AI at the table yet" } },
        n ? h("span", { class: "seat-logos" }, heads.map((hd) => headLogo(hd, "sm"))) : null,
        est,
      ),
      h("div", { class: "actions" }, n === 0 ? button("Open Settings", () => void app.openSettings(), "btn") : null, go),
    ),
    ask.rounds > 1 && chairMissing ? h("p", { class: "small muted", text: "Two or more rounds work best with Claude connected, because Claude is the chair." }) : null,
  );
}

/** Rounds: 1 to 4. One round is independent answers; more rounds talk it through. */
function roundsControl(app: App): HTMLElement {
  const ask = app.state.ask;
  return h(
    "div",
    { class: "ask-field" },
    h("span", { class: "field-label", attrs: { id: "rounds-label" }, text: "Rounds" }),
    h(
      "div",
      { class: "seg", role: "radiogroup", attrs: { "aria-labelledby": "rounds-label" } },
      Array.from({ length: MAX_ROUNDS }, (_, i) => i + 1).map((r) =>
        h("button", {
          type: "button",
          class: `seg-btn${ask.rounds === r ? " on" : ""}`,
          text: String(r),
          attrs: { role: "radio", "aria-checked": String(ask.rounds === r), "data-rounds": String(r), "aria-label": `${r} round${r === 1 ? "" : "s"}` },
          on: { click: () => app.set({ ask: { ...ask, rounds: r } }) },
        }),
      ),
    ),
    h("small", { class: "muted field-hint", text: roundsHint(ask.rounds) }),
  );
}

/** The Seats choices: Generic, each preset, each whole pack, or every seat by hand. */
export function seatOptions(lib: PersonaLibrary | null): { value: string; label: string; group?: string }[] {
  const out: { value: string; label: string; group?: string }[] = [{ value: "", label: "Generic: one plain seat per AI" }];
  for (const p of lib?.packs ?? []) {
    for (const pr of p.presets) out.push({ value: `preset:${pr.id}`, label: `${pr.name} (${pr.seats.length})`, group: p.name });
    if (p.personas.length) out.push({ value: `pack:${p.id}`, label: `Every seat in ${p.name} (${p.personas.length})`, group: p.name });
  }
  out.push({ value: "custom", label: "Set each seat" });
  return out;
}

export function select(label: string, value: string, options: { value: string; label: string; group?: string }[], onChange: (v: string) => void, cls = ""): HTMLSelectElement {
  const groups = new Map<string, HTMLElement>();
  const el = h("select", { class: `select ${cls}`.trim(), attrs: { "aria-label": label }, on: { change: (e) => onChange((e.target as HTMLSelectElement).value) } });
  for (const o of options) {
    const opt = h("option", { value: o.value, text: o.label });
    if (o.group) {
      let g = groups.get(o.group);
      if (!g) {
        g = h("optgroup", { attrs: { label: o.group } });
        groups.set(o.group, g);
        el.appendChild(g);
      }
      g.appendChild(opt);
    } else el.appendChild(opt);
  }
  el.value = value;
  return el;
}

function seatsControl(app: App, heads: string[], plan: SeatPick[]): HTMLElement {
  const s = app.state;
  const ask = s.ask;
  const lib = s.personas;
  const choose = (v: string) => {
    const seatList = v === "custom" ? (ask.seats === "custom" && ask.seatList.length ? ask.seatList : plan) : ask.seatList;
    app.set({ ask: { ...ask, seats: v, seatList } });
    if (v !== "custom") void app.saveSettings({ personas: v });
  };
  const personaOptions = [{ value: "", label: "Generic seat" }, ...(lib?.packs ?? []).flatMap((p) => p.personas.map((x) => ({ value: x.id, label: x.name, group: p.name })))];
  const aiOptions = heads.map((hd) => ({ value: hd, label: providerName(hd) }));
  const edit = (i: number, patch: Partial<SeatPick>) => {
    const list = ask.seatList.map((x, j) => (j === i ? { ...x, ...patch } : x));
    app.set({ ask: { ...ask, seatList: list } });
  };
  const preferred = (id: string, fallback: string) => {
    const ai = findPersona(lib, id)?.ai;
    return ai && heads.includes(ai) ? ai : fallback;
  };
  const infos = seatInfos(plan, lib);
  const ais = new Set(plan.map((x) => x.head)).size;
  const preview =
    ask.seats === "custom"
      ? h(
          "div",
          { class: "seat-edit" },
          ask.seatList.map((x, i) =>
            h(
              "div",
              { class: "seat-pick", attrs: { "data-seat": String(i) } },
              select(`Seat ${i + 1} persona`, x.persona, personaOptions, (v) => edit(i, { persona: v, head: preferred(v, x.head) })),
              h("span", { class: "seat-on muted", text: "on" }),
              select(`Seat ${i + 1} AI`, heads.includes(x.head) ? x.head : heads[0] ?? "", aiOptions, (v) => edit(i, { head: v }), "ai-select"),
              h("button", {
                type: "button",
                class: "chip-x",
                text: "×",
                disabled: ask.seatList.length <= 1,
                attrs: { "aria-label": `Remove seat ${i + 1}` },
                on: { click: () => app.set({ ask: { ...ask, seatList: ask.seatList.filter((_, j) => j !== i) } }) },
              }),
              x.persona && findPersona(lib, x.persona)?.lab ? h("small", { class: "lab-label", text: infos[i]?.label ?? "" }) : null,
            ),
          ),
          heads.length
            ? withGlyph(
                button("Add a seat", () => app.set({ ask: { ...ask, seatList: [...ask.seatList, { persona: allPersonas(lib)[0]?.id ?? "", head: heads[ask.seatList.length % heads.length] }] } }), "btn small ghost", ask.seatList.length >= 16),
                "plus",
              )
            : null,
        )
      : ask.seats
        ? h(
            "ul",
            { class: "seat-preview", attrs: { "aria-label": "Who sits where" } },
            infos.map((x) =>
              h(
                "li",
                null,
                headLogo(x.head, "sm"),
                h("span", { class: "seat-text" }, h("span", { class: "seat-name", text: x.persona ?? providerName(x.head) }), h("span", { class: "muted", text: ` on ${providerName(x.head)}` })),
                x.label ? h("small", { class: "lab-label", text: x.label }) : null,
              ),
            ),
          )
        : null;
  return h(
    "div",
    { class: "ask-field" },
    h("span", { class: "field-label", attrs: { id: "seats-label" }, text: "Seats" }),
    select("Seats", ask.seats, seatOptions(lib), choose, "seats-select"),
    h("small", { class: "muted field-hint", text: ask.seats ? `${plan.length} seat${plan.length === 1 ? "" : "s"} on ${ais} AI${ais === 1 ? "" : "s"}.` : "Each AI answers as itself." }),
    preview,
  );
}

function results(app: App, run: RunView): HTMLElement {
  const last = run.verdicts[run.verdicts.length - 1];
  const verdict =
    run.rounds > 1 && last
      ? (() => {
          const v = splitVerdict(last.text);
          return h(
            "section",
            { class: `verdict ${last.consensus ? "agree" : "disagree"}` },
            h("p", { class: "verdict-by" }, headLogo("claude", "sm"), h("span", { text: `Claude, as chair, after round ${last.round}` })),
            h("h2", { text: last.consensus ? "They agree" : run.done ? "They still differ" : `Round ${last.round}: not yet agreed` }),
            renderMarkdown(v.body),
          );
        })()
      : null;
  const progress = !run.done
    ? h("p", { class: "muted", role: "status", text: run.rounds > 1 ? `Round ${run.round} of up to ${run.rounds}...` : "Asking the table..." })
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
    h("div", { class: "answers grid" }, run.heads.map((k) => answerCard(run.cards[k] ?? { head: k, seat: k, status: "waiting", text: "", round: 1 }, app, run))),
    run.rounds > 1 && run.steps.length ? roundSteps(run) : null,
    h("p", { class: "small muted", text: `At the table: ${run.heads.map((k) => seatName(run, k)).join(", ")}.` }),
  );
}

/** "CFO on Claude", or "Claude" on a Generic seat. */
function seatName(run: RunView, key: string): string {
  const c = run.cards[key];
  const head = c?.head ?? run.seats[key]?.head ?? key;
  const p = c?.persona ?? run.seats[key]?.persona;
  return p ? `${p} on ${providerName(head)}` : providerName(head);
}

/** Every round as a step the person can open: the answers in it, then the chair's verdict. */
function roundSteps(run: RunView): HTMLElement {
  const lastRound = run.steps[run.steps.length - 1]?.round;
  return h(
    "section",
    { class: "steps", attrs: { "aria-label": "Rounds" } },
    h("h3", { text: "Round by round" }),
    run.steps.map((st: RoundStep) =>
      h(
        "details",
        { class: "step", attrs: { "data-round": String(st.round) } },
        h(
          "summary",
          null,
          h("span", { class: "step-n", text: String(st.round) }),
          h("span", { class: "step-title", text: st.round === 1 ? "First answers, each on its own" : "They read the others and revised" }),
          st.verdict
            ? h("span", { class: `state ${st.verdict.consensus ? "answered" : "differ"}`, text: st.verdict.consensus ? "Agreed" : "Still differ" })
            : h("span", { class: "state", text: st.round === lastRound && !run.done ? "Running" : "" }),
        ),
        h(
          "div",
          { class: "step-body" },
          run.heads
            .filter((k) => st.answers[k])
            .map((k) =>
              h(
                "div",
                { class: "step-answer" },
                h("p", { class: "step-who" }, headLogo(run.cards[k]?.head ?? k, "sm"), h("strong", { text: seatName(run, k) })),
                st.answers[k].text ? renderMarkdown(cleanAnswer(st.answers[k].text)) : h("p", { class: "muted", text: "No answer this round." }),
              ),
            ),
          st.verdict
            ? h("div", { class: "step-verdict" }, h("p", { class: "step-who" }, headLogo("claude", "sm"), h("strong", { text: "Chair" })), renderMarkdown(splitVerdict(st.verdict.text).body))
            : null,
        ),
      ),
    ),
  );
}

/** Puts an interface glyph before a button's words. The words stay the button's name. */
export function withGlyph(b: HTMLButtonElement, name: string): HTMLButtonElement {
  b.prepend(glyph(name));
  b.classList.add("has-glyph");
  return b;
}
