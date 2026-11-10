// The main screen: past roundtables on the left, the ask box or results in the centre.

import type { App } from "../app";
import { decisionPanel } from "../decision";
import { button, h, toggle } from "../dom";
import { renderMarkdown } from "../markdown";
import { providerName } from "../providers";
import {
  allFailed,
  allPersonas,
  cleanAnswer,
  costLine,
  estimateLine,
  deleteTargets,
  filterHistory,
  findPersona,
  firstFailure,
  flagOnly,
  firstLine,
  DEPTHS,
  depthHint,
  depthLine,
  depthTitle,
  planSeats,
  roundtables,
  seatInfos,
  shortDate,
  splitVerdict,
  statusDot,
  tableHeads,
  type RoundStep,
  type RunView,
  type SeatPick,
} from "../state";
import type { HistoryEntry, PersonaLibrary } from "../api";
import { answerCard } from "./onboarding";
import { renderCoach } from "./coach";
import { attachmentLabel, attachmentNote, confidenceOf, formatVerdictBody, seatName, splitFinalVerdict, VERDICT_SHAPES, verdictHint, verdictTitle } from "../session";
import { glyph, headLogo, lockup } from "../brand";
import { mainFooter, withCredit } from "../credits";

export function renderMain(app: App): HTMLElement {
  return h("div", { class: "shell" }, sidebar(app), h("main", { class: "centre" }, topbar(app), centre(app), mainFooter(app)));
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
  const picked = s.historySel.filter((id) => list.some((e) => e.id === id));
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
    list.length ? selectBar(app, list.length, picked) : null,
    h(
      "ul",
      { class: s.selecting ? "history selecting" : "history", attrs: { "aria-label": "Past roundtables", ...(s.selecting ? { "aria-multiselectable": "true" } : {}) } },
      list.map((e) => historyRow(app, e)),
    ),
  );
}

/** The line over the list: a count and Select, or what is picked and Delete. */
function selectBar(app: App, total: number, picked: string[]): HTMLElement {
  const s = app.state;
  if (!s.selecting) {
    return h(
      "div",
      { class: "select-bar" },
      h("span", { class: "select-count", text: roundtables(total) }),
      h("button", { class: "btn small ghost", type: "button", text: "Select", attrs: { "aria-label": "Select roundtables to delete" }, on: { click: () => app.setSelecting(true) } }),
    );
  }
  const all = picked.length === total;
  return h(
    "div",
    { class: "select-bar on", role: "toolbar", attrs: { "aria-label": "Selected roundtables" } },
    h("span", { class: "select-count", attrs: { "aria-live": "polite" }, text: picked.length ? `${picked.length} selected` : "Pick roundtables" }),
    h(
      "span",
      { class: "select-actions" },
      h("button", {
        class: "btn small ghost",
        type: "button",
        text: all ? "None" : "All",
        attrs: { "aria-label": all ? "Select none" : "Select all" },
        on: { click: () => app.set({ historySel: all ? [] : filterHistory(s.history, s.historyFilter).map((e) => e.id) }) },
      }),
      h("button", {
        class: "btn small danger",
        type: "button",
        text: "Delete",
        disabled: !picked.length,
        attrs: { "aria-label": picked.length ? `Delete ${roundtables(picked.length)}` : "Delete selected" },
        on: { click: () => void app.deleteHistory(picked) },
      }),
      h("button", { class: "btn small ghost", type: "button", text: "Done", attrs: { "aria-label": "Stop selecting" }, on: { click: () => app.setSelecting(false) } }),
    ),
  );
}

let openMenu: HTMLElement | null = null;
function closeMenu() {
  openMenu?.remove();
  openMenu = null;
}

/** Right click (or the menu key) on a row: Open, Delete, or delete what is picked. */
function rowMenu(app: App, e: HistoryEntry, x: number, y: number) {
  closeMenu();
  const targets = deleteTargets(app.state.historySel, e.id);
  const item = (text: string, label: string, run: () => void, cls = "menu-item") =>
    h("button", { type: "button", class: cls, text, attrs: { role: "menuitem", "aria-label": label }, on: { click: () => { closeMenu(); run(); } } });
  const menu = h(
    "div",
    { class: "menu", role: "menu", attrs: { "aria-label": `Roundtable: ${firstLine(e.question, 60)}` } },
    targets.length === 1 ? item("Open", `Open ${firstLine(e.question, 60)}`, () => void app.openHistory(e.id)) : null,
    item(targets.length === 1 ? "Delete" : `Delete ${roundtables(targets.length)}`, targets.length === 1 ? `Delete ${firstLine(e.question, 60)}` : `Delete ${roundtables(targets.length)}`, () => void app.deleteHistory(targets), "menu-item danger"),
  );
  menu.addEventListener("keydown", (ev) => {
    const items = [...menu.querySelectorAll<HTMLButtonElement>(".menu-item")];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (ev.key === "Escape") {
      closeMenu();
      [...document.querySelectorAll<HTMLElement>(".history li")].find((li) => li.dataset.id === e.id)?.focus();
    } else if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
      ev.preventDefault();
      items[(i + (ev.key === "ArrowDown" ? 1 : items.length - 1)) % items.length]?.focus();
    }
  });
  menu.style.left = `${Math.min(x, window.innerWidth - 200)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - 90)}px`;
  document.body.appendChild(menu);
  openMenu = menu;
  menu.querySelector<HTMLButtonElement>(".menu-item")?.focus();
  setTimeout(() => document.addEventListener("click", closeMenu, { once: true }), 0);
}

function historyRow(app: App, e: HistoryEntry): HTMLElement {
  const s = app.state;
  const picked = s.historySel.includes(e.id);
  const q = firstLine(e.question, 60);
  const cls = [s.opened?.id === e.id ? "active" : "", picked ? "picked" : ""].filter(Boolean).join(" ");
  const li = h(
    "li",
    {
      class: cls,
      attrs: {
        tabindex: "0",
        "data-id": e.id,
        "aria-label": `${q}, ${shortDate(e.ts)}`,
        ...(s.opened?.id === e.id ? { "aria-current": "true" } : {}),
        ...(s.selecting ? { "aria-selected": String(picked) } : {}),
      },
      on: {
        click: (ev) => {
          if ((ev.target as HTMLElement).closest(".row-delete, .row-check")) return;
          app.clickHistory(e.id, ev);
        },
        keydown: (ev) => {
          const rows = [...document.querySelectorAll<HTMLElement>(".history li")];
          const i = rows.indexOf(ev.currentTarget as HTMLElement);
          if (ev.target !== ev.currentTarget) return;
          if (ev.key === "Enter") app.clickHistory(e.id, ev);
          else if (ev.key === " " && s.selecting) {
            ev.preventDefault();
            app.clickHistory(e.id, { metaKey: true });
          } else if (ev.key === "Delete" || ev.key === "Backspace") {
            ev.preventDefault();
            void app.deleteHistory(deleteTargets(s.historySel, e.id));
          } else if (ev.key === "Escape" && s.selecting) app.setSelecting(false);
          else if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
            ev.preventDefault();
            const next = rows[i + (ev.key === "ArrowDown" ? 1 : -1)];
            if (next && ev.shiftKey) app.clickHistory(next.dataset.id!, { shiftKey: true });
            else next?.focus();
          }
        },
        contextmenu: (ev) => {
          ev.preventDefault();
          const r = (ev.currentTarget as HTMLElement).getBoundingClientRect();
          // The menu key sends no pointer position: open the menu at the row.
          rowMenu(app, e, ev.clientX || r.left + 24, ev.clientY || r.bottom - 4);
        },
      },
    },
    s.selecting
      ? h("input", {
          class: "row-check",
          type: "checkbox",
          checked: picked,
          attrs: { "aria-label": `Select ${q}`, tabindex: "-1" },
          on: { change: () => app.clickHistory(e.id, { metaKey: true }) },
        })
      : null,
    h("span", { class: "row-text" }, h("span", { class: "q", text: q }), h("span", { class: "meta" }, e.source === "claude" ? h("span", { class: "badge", text: "Claude" }) : null, h("span", { text: shortDate(e.ts) }))),
    s.selecting
      ? null
      : h(
          "button",
          {
            class: "row-delete",
            type: "button",
            title: "Delete",
            attrs: { "aria-label": `Delete ${q}` },
            on: { click: (ev) => { ev.stopPropagation(); void app.deleteHistory([e.id]); } },
          },
          glyph("trash"),
        ),
  );
  return li;
}

function centre(app: App): HTMLElement {
  const s = app.state;
  if (s.coach) return renderCoach(app, s.coach);
  if (s.run) return results(app, s.run);
  if (s.opened) {
    return h(
      "div",
      { class: "pane" },
      h("div", { class: "actions left" }, button("New question", () => app.newQuestion(), "btn"), button("Ask a follow-up", () => app.followUp(), "btn"), button("Copy as Markdown", () => void app.api.copyText(withCredit(s.opened!.transcript, s.settings.export_credit)).then(() => app.showToast("Copied as Markdown.")), "btn ghost"), button("Download .md", () => void app.api.saveTextFile("roundtable.md", withCredit(s.opened!.transcript, s.settings.export_credit)), "btn ghost"), deleteOpened(app)),
      h("div", { class: "transcript card" }, renderMarkdown(s.opened.transcript)),
    );
  }
  return askBox(app);
}

/** Delete the open roundtable, with Undo. */
function deleteOpened(app: App): HTMLElement {
  const b = withGlyph(button("Delete", () => void app.deleteHistory([app.state.opened!.id]), "btn ghost danger push-right"), "trash");
  b.setAttribute("aria-label", "Delete this roundtable");
  return b;
}

function askBox(app: App): HTMLElement {
  const s = app.state;
  const ask = s.ask;
  const heads = tableHeads(s.settings, s.keys, app.cliHeads());
  const n = heads.length;
  const plan = planSeats(ask, heads, s.personas);
  const est = h("p", { class: "estimate small muted", text: ask.depth === "deep" && n ? `${depthLine("deep", ask.rounds)} ${estimateLine(n, ask.rounds, plan.length)}` : estimateLine(n, ask.rounds, plan.length) });
  // The cost line, when a provider seat sits: published prices, free on this computer, else unknown.
  const costText = costLine(plan.map((p) => p.head), ask.rounds, s.providers.list);
  const cost = costText ? h("p", { class: "estimate cost small muted", attrs: { "data-cost": "" }, text: costText }) : null;
  const deep = ask.depth === "deep";
  const go = button(deep ? "Sharpen and think deeply" : ask.coach ? "Sharpen and ask" : "Ask the table", () => void app.startAsk(), "btn primary large", n === 0 || flagOnly(ask.question));
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
          go.disabled = n === 0 || flagOnly(ask.question);
        },
        keydown: (e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && !go.disabled) go.click();
        },
      },
    }),
    h("div", { class: "ask-grid" }, h("div", { class: "ask-col" }, depthControl(app), verdictControl(app)), seatsControl(app, heads, plan)),
    h(
      "div",
      { class: "options" },
      deep
        ? lockedToggle("Sharpen my question first", "Deep Think always sharpens the question first.")
        : toggle("Sharpen my question first", ask.coach, (v) => app.set({ ask: { ...ask, coach: v } }), "One AI asks up to three questions and writes a brief."),
      toggle("Let them search the web", ask.research, (v) => app.set({ ask: { ...ask, research: v } })),
    ),
    materialRow(app),
    h(
      "div",
      { class: "ask-bar" },
      h(
        "div",
        { class: "seat-row", attrs: { "aria-label": n ? `At the table: ${heads.map(providerName).join(", ")}` : "No AI at the table yet" } },
        n ? h("span", { class: "seat-logos" }, heads.map((hd) => headLogo(hd, "sm"))) : null,
        h("span", { class: "estimate-lines" }, est, cost),
      ),
      h("div", { class: "actions" }, n === 0 ? button("Open Settings", () => void app.openSettings(), "btn") : null, go),
    ),
    ask.rounds > 1 && chairMissing ? h("p", { class: "small muted", text: "Deep Think works best with Claude connected, because Claude is the chair." }) : null,
  );
}

/** A switch that is on and cannot be turned off here, with the reason. */
function lockedToggle(label: string, why: string): HTMLElement {
  const t = toggle(label, true, () => undefined, why);
  t.classList.add("locked");
  const input = t.querySelector("input");
  if (input) input.disabled = true;
  return t;
}

/** Quick (the default) or Deep Think. Deep Think says how long it takes before it asks. */
function depthControl(app: App): HTMLElement {
  const ask = app.state.ask;
  return h(
    "div",
    { class: "ask-field" },
    h("span", { class: "field-label", attrs: { id: "depth-label" }, text: "Mode" }),
    h(
      "div",
      { class: "seg choice-seg depth-seg", role: "radiogroup", attrs: { "aria-labelledby": "depth-label" } },
      DEPTHS.map((d) =>
        h("button", {
          type: "button",
          class: `seg-btn wide${ask.depth === d.id ? " on" : ""}`,
          text: d.label,
          attrs: { role: "radio", "aria-checked": String(ask.depth === d.id), "data-depth": d.id },
          on: { click: () => app.setDepth(d.id) },
        }),
      ),
    ),
    h("small", { class: "muted field-hint", attrs: { "data-depth-hint": "" }, text: depthHint(ask.depth) }),
  );
}

/** The verdict shape: decision, pros and cons, action plan, or vote. */
function verdictControl(app: App): HTMLElement {
  const ask = app.state.ask;
  return h(
    "div",
    { class: "ask-field" },
    h("span", { class: "field-label", attrs: { id: "verdict-label" }, text: "Verdict" }),
    select("Verdict", ask.verdict, VERDICT_SHAPES.map((v) => ({ value: v.id, label: v.label })), (v) => {
      app.set({ ask: { ...ask, verdict: v as typeof ask.verdict } });
      void app.saveSettings({ verdict_shape: v as typeof ask.verdict });
    }, "verdict-select"),
    h("small", { class: "muted field-hint", text: `${verdictHint(ask.verdict)} It names the minority view too.` }),
  );
}

/** Files and links every seat reads: read once, long ones summarized once. */
function materialRow(app: App): HTMLElement {
  const ask = app.state.ask;
  const linkInput = h("input", {
    type: "text",
    class: "link-input",
    placeholder: "Paste a link",
    value: ask.link,
    attrs: { "aria-label": "A link every seat reads", spellcheck: "false" },
    on: {
      input: (e) => {
        ask.link = (e.target as HTMLInputElement).value;
      },
      keydown: (e) => {
        if (e.key === "Enter") void app.addLink();
      },
    },
  });
  return h(
    "section",
    { class: "material", attrs: { "aria-label": "Files and links" } },
    h("div", { class: "material-head" }, h("span", { class: "field-label", text: "Files and links" }), h("small", { class: "muted", text: "Optional. Text, PDF or a web page. Every seat reads the same copy." })),
    ask.contextName
      ? h(
          "div",
          { class: "attachment" },
          glyph("file"),
          h("span", { class: "att-text" }, h("span", { class: "att-name", text: `Following up on: ${ask.contextName}` }), h("small", { class: "muted", text: "Every seat reads the earlier roundtable first." })),
          h("button", { type: "button", class: "chip-x", text: "×", attrs: { "aria-label": "Stop following up" }, on: { click: () => { app.state.priorFromHistory = null; app.set({ ask: { ...ask, contextName: null, contextPath: null, contextText: null } }); } } }),
        )
      : null,
    ask.attachments.length
      ? h(
          "ul",
          { class: "attachments" },
          ask.attachments.map((a, i) =>
            h(
              "li",
              { class: "attachment", attrs: { "data-kind": a.kind } },
              glyph(a.kind === "link" ? "link" : "file"),
              h("span", { class: "att-text" }, h("span", { class: "att-name", text: attachmentLabel(a), title: a.name }), h("small", { class: "muted", text: attachmentNote(a) })),
              h("button", { type: "button", class: "chip-x", text: "×", attrs: { "aria-label": `Remove ${attachmentLabel(a)}` }, on: { click: () => app.removeAttachment(i) } }),
            ),
          ),
        )
      : null,
    h(
      "div",
      { class: "material-add" },
      withGlyph(button("Add a file", () => void app.pickAttachment(), "btn small", ask.attaching), "file"),
      h("div", { class: "link-row" }, linkInput, button("Add link", () => void app.addLink(), "btn small", ask.attaching)),
      ask.attaching ? h("span", { class: "small muted working", role: "status", text: "Reading..." }) : null,
    ),
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
  const chairCard =
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
  const waitingVerdict = run.verdictShape && !run.verdict && !run.done && Object.values(run.cards).every((c) => c.status !== "thinking" && c.status !== "waiting") && run.round >= 1;
  const progress = !run.done
    ? h("p", { class: "muted", role: "status", text: `${run.depth === "deep" ? "Deep Think. " : ""}${waitingVerdict ? `The chair is writing the ${run.depth === "deep" ? "full " : ""}verdict (${verdictTitle(run.verdictShape!).toLowerCase()})...` : run.rounds > 1 ? `Round ${run.round} of up to ${run.rounds}...` : "Asking the table..."}` })
    : null;
  const turns = app.state.session;
  return h(
    "div",
    { class: "pane results" },
    turns.length ? pastTurns(turns) : null,
    run.target && run.target !== "table" ? h("p", { class: "turn-tag small muted", text: `Follow-up to ${seatName(run, run.target)}` }) : turns.length ? h("p", { class: "turn-tag small muted", text: "Follow-up to the whole table" }) : null,
    h("h2", { class: "q-title", text: run.question }),
    h(
      "div",
      { class: "actions left" },
      run.done
        ? [
            // With a verdict, these sit on the verdict card.
            run.verdict ? null : button("Copy as Markdown", () => void app.copyMarkdown(run), "btn"),
            run.verdict ? null : button("Download .md", () => void app.downloadMarkdown(run), "btn"),
            turns.length ? button("Export session", () => void app.exportSession(), "btn") : null,
            button("New question", () => app.newQuestion(), "btn ghost"),
          ]
        : button("Stop", () => void app.stop(), "btn danger"),
    ),
    run.brief ? briefCard(run.brief) : null,
    run.attachments?.length ? h("p", { class: "small muted", text: `Every seat read: ${run.attachments.join(", ")}.` }) : null,
    progress,
    run.stopped ? h("p", { class: "status bad", text: "Stopped." }) : null,
    run.done && !run.stopped && allFailed(run) ? h("p", { class: "status bad", role: "alert", text: firstFailure(run) }) : null,
    run.clarify?.length && !run.verdict ? clarifyCard(run.clarify) : null,
    run.verdict ? finalVerdict(app, run) : null,
    decisionPanel(run.decision),
    seatAnswers(run, [
      chairCard,
      h("div", { class: "answers grid" }, run.heads.map((k) => answerCard(run.cards[k] ?? { head: k, seat: k, status: "waiting", text: "", round: 1 }, app, run))),
      run.rounds > 1 && run.steps.length ? roundSteps(run) : null,
    ]),
    h("p", { class: "small muted", text: `At the table: ${run.heads.map((k) => seatName(run, k)).join(", ")}.${run.openFirstRound ? " Round 1 was open: each seat read the seats before it." : ""}` }),
    run.done && !run.stopped && !allFailed(run) ? followUpBox(app, run) : null,
  );
}

/** The seat answers: open while the table answers, folded under the verdict once it lands. */
export function seatAnswers(run: RunView, body: (HTMLElement | null)[]): HTMLElement {
  if (!run.verdict) return h("div", { class: "seat-answers" }, body);
  const n = run.heads.length;
  const rounds = run.steps.length > 1 ? `, ${run.steps.length} rounds` : "";
  const fold = h(
    "details",
    { class: "seat-answers seat-fold" },
    h("summary", null, h("span", { class: "field-label", text: "The seat answers" }), h("span", { class: "small muted", text: `${n} seat${n === 1 ? "" : "s"}${rounds}. The verdict above sums them up.` })),
    body,
  );
  // Stays open across redraws once the person opens it.
  fold.open = openFold === run.question;
  fold.addEventListener("toggle", () => {
    openFold = fold.open ? run.question : openFold === run.question ? null : openFold;
  });
  return fold;
}

/** The question whose folded seat answers the person opened. */
let openFold: string | null = null;

/** The question lacked key facts: what the chair asks first, while the table answers. */
function clarifyCard(questions: string[]): HTMLElement {
  return h(
    "section",
    { class: "clarify-card", attrs: { "aria-label": "The chair asks first" } },
    h("span", { class: "field-label", text: "The chair asks first" }),
    h("ol", null, questions.map((q) => h("li", { text: q }))),
    h("small", { class: "muted", text: "The table answers with these open. Answer them in a follow-up to sharpen the pick." }),
  );
}

function briefCard(md: string): HTMLElement {
  return h("details", { class: "brief-card" }, h("summary", null, h("span", { class: "field-label", text: "The brief" }), h("span", { class: "small muted", text: "What every seat read before your question" })), renderMarkdown(md));
}

/** The chair's final verdict, in its shape, with the minority view and who held it. */
function finalVerdict(app: App, run: RunView): HTMLElement {
  const v = run.verdict!;
  const parts = splitFinalVerdict(v.text);
  const minority = v.minority ?? parts.minority;
  const heldBy = v.held_by ?? parts.heldBy;
  const conf = v.shape === "decision" ? confidenceOf(v) : undefined;
  const unavailable = /verdict unavailable/i.test(v.text);
  return h(
    "section",
    { class: `final-verdict${unavailable ? " unavailable" : ""}`, attrs: { "data-shape": v.shape, "aria-label": "Verdict" } },
    h(
      "div",
      { class: "fv-head" },
      h("p", { class: "verdict-by fv-by" }, headLogo("claude", "sm"), h("span", { text: "Claude, as chair, final verdict" })),
      h("span", { class: `state depth-tag${run.depth === "deep" ? " deep" : ""}`, attrs: { "data-depth": run.depth ?? "quick" }, text: depthTitle(run.depth) }),
      conf ? h("span", { class: `state conf-${conf}`, text: `${conf[0].toUpperCase()}${conf.slice(1)} confidence` }) : null,
    ),
    h("h2", { text: verdictTitle(v.shape) }),
    unavailable ? h("p", { class: "muted", text: "The chair did not answer, so there is no verdict. Connect Claude to get one." }) : renderMarkdown(formatVerdictBody(parts.body)),
    unavailable
      ? null
      : h(
          "div",
          { class: "minority" },
          h("span", { class: "field-label", text: "Minority view" }),
          minority
            ? h("p", null, h("span", { text: minority }), h("span", { class: "held-by", text: ` Held by ${heldBy ?? "a seat the chair did not name"}.` }))
            : h("p", { class: "muted", text: "None. Every seat backed the verdict." }),
        ),
    h(
      "div",
      { class: "actions left fv-actions" },
      withGlyph(button("Copy as Markdown", () => void app.copyMarkdown(run), "btn small"), "copy"),
      withGlyph(button("Download .md", () => void app.downloadMarkdown(run), "btn small"), "download"),
    ),
  );
}

/** Earlier turns of this session, folded. */
function pastTurns(turns: RunView[]): HTMLElement {
  return h(
    "section",
    { class: "turns", attrs: { "aria-label": "Earlier in this session" } },
    turns.map((t, i) =>
      h(
        "details",
        { class: "turn" },
        h(
          "summary",
          null,
          h("span", { class: "step-n", text: String(i + 1) }),
          h("span", { class: "turn-q", text: t.question }),
          h("span", { class: "small muted", text: t.target && t.target !== "table" ? seatName(t, t.target) : t.verdict ? verdictTitle(t.verdict.shape) : `${t.heads.length} seats` }),
        ),
        h(
          "div",
          { class: "step-body" },
          t.verdict ? renderMarkdown(splitFinalVerdict(t.verdict.text).body) : null,
          t.heads
            .filter((k) => t.cards[k]?.text)
            .map((k) => h("div", { class: "step-answer" }, h("p", { class: "step-who" }, headLogo(t.cards[k].head, "sm"), h("strong", { text: seatName(t, k) })), renderMarkdown(cleanAnswer(t.cards[k].text)))),
        ),
      ),
    ),
  );
}

/** Ask the whole table a follow-up, or one seat. The session's history goes with it. */
function followUpBox(app: App, run: RunView): HTMLElement {
  const s = app.state;
  let target = "table";
  const input = h("textarea", {
    class: "question",
    rows: 2,
    placeholder: "Ask a follow-up",
    value: s.followUpText,
    attrs: { "aria-label": "Your follow-up" },
    on: {
      input: (e) => {
        s.followUpText = (e.target as HTMLTextAreaElement).value;
        go.disabled = !s.followUpText.trim();
      },
      keydown: (e) => {
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && s.followUpText.trim()) void app.askFollowUp(s.followUpText, target);
      },
    },
  });
  const go = button("Ask", () => void app.askFollowUp(s.followUpText, target), "btn primary", !s.followUpText.trim());
  const options = [{ value: "table", label: "The whole table" }, ...run.heads.map((k) => ({ value: k, label: `Only ${seatName(run, k)}` }))];
  return h(
    "section",
    { class: "follow-up", attrs: { "aria-label": "Follow-up" } },
    h("div", { class: "follow-head" }, h("h3", { text: "Follow up" }), h("small", { class: "muted", text: "They read everything said so far." })),
    input,
    h("div", { class: "follow-bar" }, select("Who answers", target, options, (v) => (target = v), "follow-target"), go),
  );
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
