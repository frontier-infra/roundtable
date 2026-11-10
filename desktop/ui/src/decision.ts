// The decision model: a System One model (Jev, Jebadiah, or another such as LINK21)
// rates the options the table argued. The engine does the work after the final round;
// this module holds the types, the verdict panel and the Settings section.

import type { App } from "./app";
import { button, h } from "./dom";
import { getJebadiah } from "./jebadiah";

export type DecisionProvider = "jev" | "jebadiah" | "systemone";

/** `settings.decision`. `provider: null` means off (the default). */
export interface DecisionSettings {
  provider: DecisionProvider | null;
  url: string | null;
  model: string | null;
  /** Display name for the "other" slot, for example LINK21. */
  name: string | null;
}

export interface DecisionOption {
  key: string;
  text: string;
}

/** The engine's `decision` event. */
export type DecisionEvent =
  | {
      event: "decision";
      status: "rated";
      provider: string;
      model: string;
      endpoint: string;
      options: DecisionOption[];
      probabilities: number[];
      confidence: number | null;
      top: string;
      table_pick?: string;
      disagrees: boolean;
      line: string;
      disagreement?: string;
    }
  | { event: "decision"; status: "unavailable"; provider: string; message: string };

export function defaultDecision(): DecisionSettings {
  return { provider: null, url: null, model: null, name: null };
}

export const MORE_INFO = "more_information";

function optionLabel(key: string): string {
  return key === MORE_INFO ? "More information" : `Option ${key}`;
}

/** The Markdown export's section. Same text as the engine's transcript section. */
export function decisionMarkdown(d: DecisionEvent): string {
  if (d.status === "unavailable") return `## Decision model\n\nDecision model: ${d.provider} did not answer. ${d.message}\n`;
  const out = ["## Decision model", "", d.line, ""];
  d.options.forEach((o, i) => out.push(`- **${optionLabel(o.key)}** (${(d.probabilities[i] ?? 0).toFixed(2)}): ${o.text}`));
  out.push("");
  if (d.disagreement) out.push(`**${d.disagreement}**`, "");
  out.push(`_Asked ${d.provider} (${d.model}). Probabilities are the decision model's, not the table's._`);
  return out.join("\n") + "\n";
}

/** Shown under the chair's verdict. */
export function decisionPanel(d: DecisionEvent | undefined): HTMLElement | null {
  if (!d) return null;
  if (d.status === "unavailable") {
    return h("section", { class: "decision" }, h("p", { class: "small muted", text: `Decision model: ${d.provider} did not answer. ${d.message}` }));
  }
  return h(
    "section",
    { class: `decision${d.disagrees ? " disagrees" : ""}`, attrs: { "aria-label": "Decision model" } },
    h("p", { class: "decision-line", text: d.line }),
    h(
      "ul",
      { class: "decision-bars" },
      d.options.map((o, i) => {
        const p = d.probabilities[i] ?? 0;
        const bar = h("span", { class: "bar" });
        bar.style.width = `${Math.round(p * 100)}%`;
        return h(
          "li",
          { class: o.key === d.top ? "top" : "" },
          h("span", { class: "label", text: `${optionLabel(o.key)} ${p.toFixed(2)}` }),
          h("span", { class: "track" }, bar),
          h("span", { class: "text small", text: o.text }),
        );
      }),
    ),
    d.disagreement ? h("p", { class: "decision-warn", role: "note", text: d.disagreement }) : null,
  );
}

const CHOICES: { id: DecisionProvider | "off"; label: string; hint: string }[] = [
  { id: "off", label: "Off", hint: "The table's verdict only." },
  { id: "jev", label: "Jev", hint: "TypeSafe's hosted decision model. Needs your Jev key." },
  { id: "jebadiah", label: "Jebadiah", hint: "Runs on your computer. Roundtable looks for it (jeb serve, AINode), or enter its address." },
  { id: "systemone", label: "Another System One model", hint: "Any model with the System One API, for example LINK21. Enter its address and model id." },
];

/** The Settings section. */
export function decisionSection(app: App): HTMLElement {
  const d: DecisionSettings = { ...defaultDecision(), ...(app.state.settings.decision ?? {}) };
  const save = (patch: Partial<DecisionSettings>) => void app.saveSettings({ decision: { ...d, ...patch } });
  const field = (label: string, value: string | null, placeholder: string, key: "url" | "model" | "name") =>
    h(
      "label",
      { class: "field" },
      h("span", { text: label }),
      h("input", {
        type: "text",
        value: value ?? "",
        placeholder,
        attrs: { spellcheck: "false" },
        on: { change: (e) => save({ [key]: (e.target as HTMLInputElement).value.trim() || null }) },
      }),
    );
  let draft = "";
  const keyRow =
    d.provider === "jev" || d.provider === "systemone"
      ? h(
          "div",
          { class: "row" },
          h("input", {
            type: "password",
            placeholder: d.provider === "jev" ? "Jev key (TypeSafe)" : "Key, if the model needs one",
            attrs: { autocomplete: "off", "aria-label": "Decision model key" },
            on: { input: (e) => (draft = (e.target as HTMLInputElement).value) },
          }),
          button("Save key", () => {
            if (!draft.trim()) return;
            void app.api.keySave(d.provider!, draft).then((r) => app.showToast(r.ok ? "Saved." : r.message ?? "Could not save the key."));
          }, "btn small"),
        )
      : null;
  return h(
    "section",
    { class: "setting" },
    h("h2", { text: "Decision model" }),
    h(
      "div",
      { class: "setting-body" },
      h("p", { class: "small muted", text: "After the last round, a decision model rates each option the table argued, plus \"need more information\". It says so when it disagrees with the table." }),
      h(
        "div",
        { class: "stack", attrs: { role: "radiogroup", "aria-label": "Decision model" } },
        CHOICES.map((c) =>
          h(
            "label",
            { class: "switch-row" },
            h("span", { class: "switch-text" }, h("span", { text: c.label }), h("small", { text: c.hint })),
            h("input", {
              type: "radio",
              checked: (d.provider ?? "off") === c.id,
              attrs: { name: "decision-provider" },
              on: { change: () => save({ provider: c.id === "off" ? null : c.id, url: null, model: null }) },
            }),
          ),
        ),
      ),
      keyRow,
      d.provider === "jebadiah" ? h("div", { class: "grid2" }, field("Address (optional)", d.url, "Found on this computer", "url"), field("Model id (optional)", d.model, "What the server lists", "model")) : null,
      d.provider === "systemone"
        ? h("div", { class: "grid2" }, field("Name", d.name, "LINK21", "name"), field("Address", d.url, "http://127.0.0.1:8100", "url"), field("Model id", d.model, "model id", "model"))
        : null,
      // Off or Jebadiah: what decision detect found on this computer, or how to get Jebadiah.
      d.provider === null || d.provider === "jebadiah" ? h("div", { class: "jebadiah-local" }, getJebadiah(app)) : null,
    ),
  );
}
