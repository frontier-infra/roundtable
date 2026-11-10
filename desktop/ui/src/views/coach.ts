// The question coach: up to three clarifying questions, then the brief and the suggested seats.

import type { App } from "../app";
import { button, h } from "../dom";
import { providerName } from "../providers";
import { BRIEF_FIELDS } from "../session";
import { allPersonas, depthLine, findPersona, labLabel, tableHeads, type CoachView, type SeatPick } from "../state";
import { headLogo } from "../brand";
import { select, withGlyph } from "./main";

export function renderCoach(app: App, c: CoachView): HTMLElement {
  return h(
    "div",
    { class: "pane coach", attrs: { "data-stage": c.stage } },
    h(
      "ol",
      { class: "coach-steps", attrs: { "aria-label": "Coach steps" } },
      h("li", { class: c.stage === "questions" ? "current" : "done" }, h("span", { class: "step-n", text: "1" }), h("span", { text: "Questions" })),
      h("li", { class: c.stage === "brief" ? "current" : "" }, h("span", { class: "step-n", text: "2" }), h("span", { text: "Brief and seats" })),
      h("li", null, h("span", { class: "step-n", text: "3" }), h("span", { text: "The table" })),
    ),
    h("p", { class: "coach-q" }, h("span", { class: "coach-q-label", text: "Your question" }), h("span", { text: c.question })),
    c.stage === "questions" ? questions(app, c) : brief(app, c),
  );
}

function questions(app: App, c: CoachView): HTMLElement {
  if (c.loading) {
    return h(
      "div",
      { class: "coach-body" },
      h("h1", { class: "ask-title", text: "Sharpening your question" }),
      h("p", { class: "muted working", role: "status", text: c.questions.length ? "Writing the brief..." : "The coach is reading your question..." }),
      h("div", { class: "actions left" }, button("Skip the coach", () => app.skipCoach(), "btn ghost")),
    );
  }
  if (c.error) return coachError(app, c.error);
  return h(
    "div",
    { class: "coach-body" },
    h("h1", { class: "ask-title", text: `${c.questions.length === 1 ? "One question" : `${c.questions.length} questions`} before the table meets` }),
    h("p", { class: "muted", text: "Short answers are fine. Leave one empty to skip it." }),
    h(
      "div",
      { class: "coach-qs" },
      c.questions.map((q, i) =>
        h(
          "label",
          { class: "coach-qa", attrs: { "data-q": String(i) } },
          h("span", { class: "coach-qa-q" }, h("span", { class: "step-n", text: String(i + 1) }), h("span", { text: q })),
          h("textarea", {
            rows: 2,
            value: c.answers[i] ?? "",
            placeholder: "Your answer",
            attrs: { "aria-label": q },
            on: {
              input: (e) => {
                c.answers[i] = (e.target as HTMLTextAreaElement).value;
              },
              keydown: (e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void app.writeBrief();
              },
            },
          }),
        ),
      ),
    ),
    h(
      "div",
      { class: "ask-bar" },
      h("p", { class: "small muted", text: "The coach turns your answers into a brief you can edit." }),
      h("div", { class: "actions" }, button("Skip the coach", () => app.skipCoach(), "btn ghost"), button("Write the brief", () => void app.writeBrief(), "btn primary large")),
    ),
  );
}

function coachError(app: App, msg: string): HTMLElement {
  return h(
    "div",
    { class: "coach-body" },
    h("h1", { class: "ask-title", text: "The coach is not available" }),
    h("p", { class: "status bad", role: "alert", text: msg }),
    h(
      "div",
      { class: "actions left" },
      button("Ask the table as typed", () => app.skipCoach(), "btn primary"),
      button("Try again", () => void app.openCoach(app.state.coach!.question), "btn"),
      button("Back", () => app.closeCoach(), "btn ghost"),
    ),
  );
}

function brief(app: App, c: CoachView): HTMLElement {
  if (c.loading) return questions(app, c);
  return h(
    "div",
    { class: "coach-body" },
    h("h1", { class: "ask-title", text: "The brief the table will read" }),
    c.error ? h("p", { class: "status bad", role: "alert", text: `${c.error} Edit the brief yourself, or skip the coach.` }) : h("p", { class: "muted", text: "Edit anything. Every seat reads this brief before your question." }),
    h(
      "div",
      { class: "brief-form" },
      BRIEF_FIELDS.map((f) =>
        h(
          "label",
          { class: "brief-field", attrs: { "data-field": f.key } },
          h("span", { class: "field-label", text: f.label }),
          h("textarea", {
            rows: 2,
            value: c.brief[f.key] ?? "",
            placeholder: f.placeholder,
            attrs: { "aria-label": f.label },
            on: {
              input: (e) => {
                c.brief = { ...c.brief, [f.key]: (e.target as HTMLTextAreaElement).value };
              },
            },
          }),
        ),
      ),
    ),
    deepSuggestion(app, c),
    panel(app, c),
    h(
      "div",
      { class: "ask-bar" },
      h("p", { class: "small muted", text: c.useSuggested && c.seats?.length ? `${c.seats.length} suggested seats.` : "Your Seats choice from the ask box." }),
      h(
        "div",
        { class: "actions" },
        button("Back", () => (c.questions.length ? app.set({ coach: { ...c, stage: "questions" } }) : app.closeCoach()), "btn ghost"),
        button("Skip the coach", () => app.skipCoach(), "btn ghost"),
        button("Ask the table", () => void app.acceptBrief(), "btn primary large"),
      ),
    ),
  );
}

/** The coach suggests Deep Think on a complex brief. The person takes it or keeps Quick. */
export function deepSuggestion(app: App, c: CoachView): HTMLElement | null {
  const deep = app.state.ask.depth === "deep";
  if (deep) {
    return h("p", { class: "small muted", attrs: { "data-deep-on": "" }, text: `${depthLine("deep", app.state.ask.rounds)} The table argues it out over up to ${app.state.ask.rounds} rounds and the chair writes a long verdict.` });
  }
  if (!c.brief.deep || c.deepAnswered) return null;
  return h(
    "section",
    { class: "deep-suggest", attrs: { "aria-label": "The coach suggests Deep Think" } },
    h("span", { class: "field-label", text: "The coach suggests Deep Think" }),
    h("p", { text: `${c.brief.deep_why?.trim() || "This one looks complex."} ${depthLine("deep")}` }),
    h(
      "div",
      { class: "actions left" },
      button("Use Deep Think", () => app.answerDeep(true), "btn small primary"),
      button("Keep Quick", () => app.answerDeep(false), "btn small ghost"),
    ),
  );
}

/** The suggested panel: seats from the brief. The person keeps, changes or declines them. */
function panel(app: App, c: CoachView): HTMLElement {
  const s = app.state;
  const lib = s.personas;
  const heads = tableHeads(s.settings, s.keys, app.cliHeads());
  const set = (patch: Partial<CoachView>) => app.set({ coach: { ...c, ...patch } });
  const edit = (i: number, patch: Partial<SeatPick>) => set({ seats: (c.seats ?? []).map((x, j) => (j === i ? { ...x, ...patch } : x)), useSuggested: true });
  const personaOptions = (lib?.packs ?? []).flatMap((p) => p.personas.map((x) => ({ value: x.id, label: x.name, group: p.name })));
  const aiOptions = heads.map((hd) => ({ value: hd, label: providerName(hd) }));
  let body: HTMLElement;
  if (c.suggesting || c.seats === null) {
    body = h("p", { class: "muted working", role: "status", text: "Choosing seats for this brief..." });
  } else if (!c.seats.length) {
    body = h("p", { class: "muted", text: c.suggestion?.why ?? "No persona fits closely, so each AI answers as itself." });
  } else {
    body = h(
      "div",
      { class: "panel-body" },
      h(
        "div",
        { class: "seg choice-seg", role: "radiogroup", attrs: { "aria-label": "Which seats" } },
        h("button", { type: "button", class: `seg-btn wide${c.useSuggested ? " on" : ""}`, text: "Suggested seats", attrs: { role: "radio", "aria-checked": String(c.useSuggested) }, on: { click: () => set({ useSuggested: true }) } }),
        h("button", { type: "button", class: `seg-btn wide${!c.useSuggested ? " on" : ""}`, text: "My seats", attrs: { role: "radio", "aria-checked": String(!c.useSuggested) }, on: { click: () => set({ useSuggested: false }) } }),
      ),
      c.suggestion?.why ? h("p", { class: "small muted", text: c.suggestion.why }) : null,
      h(
        "div",
        { class: `seat-edit${c.useSuggested ? "" : " off"}` },
        c.seats.map((x, i) => {
          const p = findPersona(lib, x.persona);
          return h(
            "div",
            { class: "seat-pick", attrs: { "data-seat": String(i) } },
            select(`Suggested seat ${i + 1} persona`, x.persona, personaOptions, (v) => {
              const ai = findPersona(lib, v)?.ai;
              edit(i, { persona: v, head: ai && heads.includes(ai) ? ai : x.head });
            }),
            h("span", { class: "seat-on muted", text: "on" }),
            select(`Suggested seat ${i + 1} AI`, heads.includes(x.head) ? x.head : heads[0] ?? "", aiOptions, (v) => edit(i, { head: v }), "ai-select"),
            h("button", {
              type: "button",
              class: "chip-x",
              text: "×",
              disabled: c.seats!.length <= 1,
              attrs: { "aria-label": `Remove suggested seat ${i + 1}` },
              on: { click: () => set({ seats: c.seats!.filter((_, j) => j !== i) }) },
            }),
            p?.lab ? h("small", { class: "lab-label", text: labLabel(p.lab) }) : null,
          );
        }),
        heads.length
          ? withGlyph(
              button("Add a seat", () => set({ useSuggested: true, seats: [...(c.seats ?? []), { persona: allPersonas(lib)[0]?.id ?? "", head: heads[(c.seats?.length ?? 0) % heads.length] }] }), "btn small ghost", (c.seats?.length ?? 0) >= 16),
              "plus",
            )
          : null,
      ),
    );
  }
  return h(
    "section",
    { class: "panel", attrs: { "aria-label": "Suggested seats" } },
    h("div", { class: "panel-head" }, h("h2", { text: "Suggested seats" }), c.seats?.length ? h("span", { class: "seat-logos" }, [...new Set(c.seats.map((x) => x.head))].map((hd) => headLogo(hd, "sm"))) : null),
    body,
  );
}
