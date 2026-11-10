// Settings, Personas: the lab pack switch, the person's own personas (create, edit,
// duplicate, delete, export, import) and the built-in packs to copy from.

import type { App } from "../app";
import type { Pack, Persona } from "../api";
import { button, h, toggle } from "../dom";
import { HEAD_ORDER, providerName } from "../providers";
import { labLabel } from "../state";
import { select } from "./main";

function row(app: App, p: Persona, own: boolean): HTMLElement {
  const ai = p.ai ? `Prefers ${providerName(p.ai)}` : "Any AI";
  return h(
    "li",
    { class: "persona-row", attrs: { "data-persona": p.id } },
    h(
      "div",
      { class: "persona-text" },
      h("strong", { text: p.name }),
      h("span", { class: "muted", text: p.expertise || "No expertise line yet." }),
      p.lab ? h("small", { class: "lab-label", text: labLabel(p.lab) }) : null,
    ),
    h("span", { class: "persona-ai small muted", text: ai }),
    h(
      "div",
      { class: "persona-actions" },
      own ? button("Edit", () => app.set({ personaEdit: { ...p } }), "btn small") : null,
      button("Duplicate", () => void app.duplicatePersona(p.id), "btn small ghost"),
      own
        ? button("Delete", () => {
            if (confirm(`Delete ${p.name}? This cannot be undone.`)) void app.deletePersona(p.id);
          }, "btn small ghost danger")
        : null,
    ),
  );
}

function editor(app: App, p: Persona): HTMLElement {
  const draft = { ...p };
  const field = (label: string, el: HTMLElement) => h("label", { class: "field" }, h("span", { text: label }), el);
  const input = (key: "name" | "expertise", placeholder: string) =>
    h("input", { type: "text", value: draft[key], placeholder, attrs: { spellcheck: "false" }, on: { input: (e) => (draft[key] = (e.target as HTMLInputElement).value) } });
  return h(
    "div",
    { class: "persona-editor", attrs: { "aria-label": draft.id ? `Edit ${draft.name}` : "New persona" } },
    h("h3", { text: draft.id ? `Edit ${draft.name}` : "New persona" }),
    field("Name", input("name", "Head of Support")),
    field("Expertise, one line", input("expertise", "Customers after they buy")),
    field(
      "Brief: how this seat reasons, what it weighs, what it doubts",
      h("textarea", { rows: 4, value: draft.brief, placeholder: "Reason from the support queue. Weigh time to answer and repeat contacts. Be skeptical of features nobody asked for.", on: { input: (e) => (draft.brief = (e.target as HTMLTextAreaElement).value) } }),
    ),
    field(
      "Preferred AI",
      select("Preferred AI", draft.ai ?? "", [{ value: "", label: "Any AI" }, ...HEAD_ORDER.map((id) => ({ value: id, label: providerName(id) }))], (v) => (draft.ai = v || undefined)),
    ),
    draft.lab ? h("p", { class: "lab-label", text: labLabel(draft.lab) }) : null,
    h("div", { class: "actions left" }, button("Save", () => void app.savePersona(draft), "btn primary"), button("Cancel", () => app.set({ personaEdit: null }), "btn ghost")),
  );
}

export function personasSection(app: App): HTMLElement[] {
  const s = app.state;
  const lib = s.personas;
  if (!lib) return [h("p", { class: "muted", text: "Personas load once Roundtable is set up." })];
  const own = lib.packs.find((p) => p.id === lib.custom_pack);
  const builtins = lib.packs.filter((p) => p.builtin);
  const others = lib.packs.filter((p) => !p.builtin && p.id !== lib.custom_pack);
  const out: (HTMLElement | null)[] = [
    h("p", { class: "small muted", text: "A persona is a role a seat plays: a name, a one-line expertise and a short brief. Pick them in Seats when you ask." }),
    lib.lab_pack_built
      ? toggle("Show the Frontier labs pack", s.settings.show_lab_pack, (v) => void app.setLabPack(v), "Seats named for a lab's public view. Simulated, not affiliated with any lab.")
      : null,
    h("h3", { text: "Your personas" }),
    s.personaEdit ? editor(app, s.personaEdit) : null,
    own?.personas.length
      ? h("ul", { class: "persona-list" }, own.personas.map((p) => row(app, p, true)))
      : h("p", { class: "small muted", text: "None yet. Make one, or duplicate a built-in one to change it." }),
    h(
      "div",
      { class: "actions left" },
      button("New persona", () => app.set({ personaEdit: { id: "", name: "", expertise: "", brief: "" } }), "btn"),
      button("Import a pack", () => void app.importPersonas(), "btn ghost"),
      button("Export yours", () => void app.exportPersonas(), "btn ghost", !own?.personas.length),
    ),
    ...[...others, ...builtins].map((p: Pack) =>
      h(
        "details",
        { class: "pack", attrs: { "data-pack": p.id } },
        h("summary", null, h("span", { class: "pack-name", text: p.name }), h("span", { class: "small muted", text: `${p.personas.length} personas${p.presets.length ? `, ${p.presets.length} presets` : ""}` })),
        p.description ? h("p", { class: "small muted", text: p.description }) : null,
        h("ul", { class: "persona-list" }, p.personas.map((x) => row(app, x, false))),
        p.presets.length
          ? h(
              "ul",
              { class: "preset-list small" },
              p.presets.map((pr) => h("li", null, h("strong", { text: pr.name }), h("span", { class: "muted", text: `: ${pr.seats.map((id) => lib.packs.flatMap((q) => q.personas).find((x) => x.id === id)?.name ?? id).join(", ")}` }))),
            )
          : null,
      ),
    ),
  ];
  return out.filter((x): x is HTMLElement => !!x);
}
