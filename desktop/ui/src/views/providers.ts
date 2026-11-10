// Settings, AI accounts: "Other providers". OpenRouter, Ollama, LM Studio, Groq, Together,
// or any OpenAI-compatible address. Every ticked model is its own seat (`provider:model`).
// Ollama and LM Studio are found the way AI apps are, and say "free, on this computer".

import type { App } from "../app";
import type { LocalProvider, ModelInfo, ProviderRow } from "../api";
import { button, h } from "../dom";
import { headLogo } from "../brand";
import { keychainName, priceText, type ProviderForm } from "../state";
import { LOCAL_NOTE, PRESET_NAMES, PRESET_ORDER } from "../providers";
import { testRow } from "./onboarding";

/** Base URLs and key needs, as desktop/engine/src/custom/mod.rs has them. */
const PRESET_INFO: Record<string, { url: string; key: boolean; keyPage?: string }> = {
  openrouter: { url: "https://openrouter.ai/api/v1", key: true, keyPage: "https://openrouter.ai/settings/keys" },
  ollama: { url: "http://127.0.0.1:11434/v1", key: false },
  lmstudio: { url: "http://127.0.0.1:1234/v1", key: false },
  groq: { url: "https://api.groq.com/openai/v1", key: true, keyPage: "https://console.groq.com/keys" },
  together: { url: "https://api.together.xyz/v1", key: true, keyPage: "https://api.together.ai/settings/api-keys" },
  custom: { url: "", key: false },
};

export function emptyForm(preset = "openrouter"): ProviderForm {
  return { preset, name: "", url: PRESET_INFO[preset]?.url ?? "", key: "", status: "editing" };
}

/** The whole "Other providers" block. */
export function providersSection(app: App): HTMLElement[] {
  const pv = app.state.providers;
  const out: HTMLElement[] = [
    h("h3", { text: "Other providers" }),
    h("p", { class: "small muted", text: "OpenRouter, Ollama, LM Studio, Groq, Together, or any OpenAI-compatible address. Every model you tick is its own seat." }),
  ];
  if (!pv.loaded) {
    out.push(h("p", { class: "muted", role: "status", text: "Looking for Ollama and LM Studio on this computer..." }));
    return out;
  }
  const added = new Set(pv.list.map((p) => p.id));
  const found = pv.found.filter((f) => !added.has(f.preset));
  const cards = [...found.map((f) => foundCard(app, f)), ...pv.list.map((p) => providerCard(app, p))];
  if (cards.length) out.push(h("div", { class: "cards provider-cards" }, cards));
  else out.push(h("p", { class: "muted", text: "No providers yet. Ollama and LM Studio show up here when they run on this computer." }));
  if (pv.error) out.push(h("p", { class: "status bad", role: "alert", text: pv.error }));
  out.push(
    pv.form
      ? addForm(app, pv.form)
      : h(
          "div",
          { class: "actions left" },
          button("Add a provider", () => {
            app.state.providers = { ...pv, form: emptyForm() };
            app.render();
          }, "btn small"),
          button(pv.loading ? "Looking..." : "Look again", () => void app.refreshProviders(), "btn small ghost", pv.loading),
        ),
  );
  return out;
}

/** Ollama or LM Studio found running, not added yet: tick a model to add it. */
function foundCard(app: App, f: LocalProvider): HTMLElement {
  return h(
    "div",
    { class: "ai-card provider-card", attrs: { "data-provider": f.preset, "data-found": "true" } },
    h(
      "div",
      { class: "ai-head" },
      headLogo(f.preset),
      h("div", { class: "ai-name" }, h("strong", { text: f.name }), h("span", { class: "muted", text: ` found on this computer` })),
    ),
    h("p", { class: "status ok", text: `${f.models.length} model${f.models.length === 1 ? "" : "s"}, ${f.note}` }),
    modelPicker(app, f.preset, f.models, [], true),
  );
}

/** One added provider: its state, its seats with Test, its models, and Change key or Remove. */
function providerCard(app: App, p: ProviderRow): HTMLElement {
  const pv = app.state.providers;
  const list = pv.models[p.id];
  const busy = pv.busy === p.id;
  const keyLine = p.local ? LOCAL_NOTE : p.has_key ? `Key saved${p.key_source === "env" ? " in the environment" : ` in ${keychainName(app.state.os)}`}.` : p.needs_key ? "Needs a key." : "No key needed.";
  const ok = p.ready;
  const ticked = p.models.map((m) => m.id);
  return h(
    "div",
    { class: `ai-card provider-card${ok ? " connected" : ""}`, attrs: { "data-provider": p.id } },
    h(
      "div",
      { class: "ai-head" },
      headLogo(p.preset),
      h("div", { class: "ai-name" }, h("strong", { text: p.name }), h("span", { class: "mono small muted", text: ` ${p.base_url}` })),
    ),
    h("p", { class: `status ${ok ? "ok" : "bad"}`, text: keyLine }),
    p.models.length
      ? h(
          "div",
          { class: "seat-list" },
          p.models.map((m) =>
            h(
              "div",
              { class: "seat-item", attrs: { "data-seat": m.seat } },
              h("span", { class: "mono", text: m.id }),
              h("span", { class: "small muted", text: p.local ? LOCAL_NOTE : priceText(m) }),
              testRow(app, m.seat, "app", m.id),
            ),
          ),
        )
      : h("p", { class: "small muted", text: "No models ticked yet." }),
    list && !list.loading
      ? list.ok
        ? modelPicker(app, p.id, list.models ?? [], ticked, p.local)
        : h("p", { class: "status bad", role: "alert", text: list.message ?? "Could not read the model list." })
      : null,
    h(
      "div",
      { class: "row" },
      list?.ok && !list.loading
        ? null
        : button(list?.loading ? "Reading models..." : "Choose models", () => void app.loadModels(p.id), "btn small", !!list?.loading || busy || (!ok && p.needs_key)),
      p.local
        ? null
        : button(p.has_key ? "Change key" : "Add key", () => {
            app.state.providers = { ...pv, form: { id: p.id, preset: p.preset, name: p.name, url: p.base_url, key: "", status: "editing" } };
            app.render();
          }, "btn small"),
      button("Remove", () => {
        if (confirm(`Remove ${p.name}, its seats and its saved key?`)) void app.removeProvider(p.id);
      }, "btn small ghost"),
    ),
  );
}

/** A search box and one check box per model. Searching hides rows in place, so the box keeps focus. */
function modelPicker(app: App, provider: string, models: ModelInfo[], ticked: string[], local: boolean): HTMLElement {
  const pv = app.state.providers;
  const q = pv.search[provider] ?? "";
  const busy = pv.busy === provider;
  const rows = models.map((m) => {
    const on = ticked.includes(m.id);
    const id = `pm-${provider}-${m.id}`.replace(/[^A-Za-z0-9_-]/g, "_");
    return h(
      "label",
      { class: `model-row${on ? " on" : ""}`, for: id, attrs: { "data-model": m.id, "data-hay": `${m.id} ${m.name ?? ""}`.toLowerCase() } },
      h("input", {
        id,
        type: "checkbox",
        checked: on,
        disabled: busy,
        attrs: { "aria-label": `Seat ${m.id}` },
        on: { change: (e) => void app.tickModel(provider, m.id, (e.target as HTMLInputElement).checked) },
      }),
      h("span", { class: "model-id mono", text: m.id }),
      h("span", { class: "model-price small muted", text: local ? LOCAL_NOTE : priceText(m) }),
      m.note ? h("span", { class: "model-note small muted", text: m.note }) : null,
    );
  });
  const listEl = h("div", { class: "model-list", role: "group", attrs: { "aria-label": `${PRESET_NAMES[provider] ?? provider} models` } }, rows);
  const empty = h("p", { class: "small muted model-empty", text: "No model matches." });
  const filter = (text: string) => {
    const words = text.toLowerCase().split(/\s+/).filter(Boolean);
    let shown = 0;
    for (const r of rows) {
      const hay = r.getAttribute("data-hay") ?? "";
      const show = words.every((w) => hay.includes(w));
      r.hidden = !show;
      if (show) shown++;
    }
    empty.hidden = shown > 0;
  };
  const search = h("input", {
    type: "search",
    class: "model-search",
    placeholder: `Search ${models.length} models`,
    value: q,
    attrs: { "aria-label": "Search models", spellcheck: "false" },
    on: {
      input: (e) => {
        const v = (e.target as HTMLInputElement).value;
        app.state.providers.search = { ...app.state.providers.search, [provider]: v };
        filter(v);
      },
    },
  });
  filter(q);
  if (!models.length) return h("p", { class: "small muted", text: "This provider lists no models." });
  return h("div", { class: "model-picker" }, search, listEl, empty);
}

/** "Add a provider": a preset, its address, and a key when it needs one. */
function addForm(app: App, f: ProviderForm): HTMLElement {
  const info = PRESET_INFO[f.preset] ?? PRESET_INFO.custom;
  const editing = !!f.id;
  const checking = f.status === "checking";
  const update = (patch: Partial<ProviderForm>, redraw = false) => {
    app.state.providers.form = { ...app.state.providers.form!, ...patch };
    if (redraw) app.render();
  };
  const presetSelect = h(
    "select",
    {
      class: "select",
      disabled: editing || checking,
      attrs: { "aria-label": "Provider" },
      on: { change: (e) => update({ ...emptyForm((e.target as HTMLSelectElement).value), status: "editing" }, true) },
    },
    PRESET_ORDER.map((id) => {
      const o = h("option", { text: id === "custom" ? "Custom (any OpenAI-compatible address)" : PRESET_NAMES[id], value: id });
      if (id === f.preset) o.selected = true;
      return o;
    }),
  );
  const field = (label: string, input: HTMLElement) => h("label", { class: "field" }, h("span", { text: label }), input);
  const keyInput = h("input", {
    type: "password",
    class: "key-input",
    placeholder: info.key ? "Paste your key here" : "Only if it needs one",
    value: f.key,
    disabled: checking,
    attrs: { "aria-label": "Provider key", autocomplete: "off", spellcheck: "false" },
    on: { input: (e) => update({ key: (e.target as HTMLInputElement).value }) },
  });
  return h(
    "div",
    { class: "provider-form", attrs: { "data-form": f.preset } },
    h("h3", { text: editing ? `Change ${f.name}` : "Add a provider" }),
    field("Provider", presetSelect),
    f.preset === "custom" && !editing
      ? field("Name", h("input", { type: "text", value: f.name, placeholder: "My proxy", disabled: checking, attrs: { "aria-label": "Provider name" }, on: { input: (e) => update({ name: (e.target as HTMLInputElement).value }) } }))
      : null,
    field(
      "Address",
      h("input", {
        type: "text",
        class: "mono",
        value: f.url,
        placeholder: "https://host/v1",
        disabled: checking,
        attrs: { "aria-label": "Base URL", spellcheck: "false" },
        on: { input: (e) => update({ url: (e.target as HTMLInputElement).value }) },
      }),
    ),
    info.key || f.preset === "custom" ? field(info.key ? "Key" : "Key (if it needs one)", keyInput) : h("p", { class: "small muted", text: `No key needed. ${LOCAL_NOTE[0].toUpperCase()}${LOCAL_NOTE.slice(1)} when it runs here.` }),
    info.keyPage
      ? h(
          "p",
          { class: "how small muted" },
          h("a", { href: info.keyPage, text: "Where do I get a key?", on: { click: (e) => { e.preventDefault(); void app.api.openUrl(info.keyPage!); } } }),
          ` Keys are saved in ${keychainName(app.state.os)}. Roundtable never shows a saved key again.`,
        )
      : null,
    f.status === "error" ? h("p", { class: "status bad", role: "alert", text: f.message ?? "That did not work." }) : null,
    checking ? h("p", { class: "status checking", role: "status", text: "Checking the address and reading its models..." }) : null,
    h(
      "div",
      { class: "row" },
      button(checking ? "Checking..." : editing ? "Check and save" : "Connect", () => void app.addProviderFromForm(), "btn small", checking),
      button("Cancel", () => {
        app.state.providers = { ...app.state.providers, form: null };
        app.render();
      }, "btn small ghost", checking),
    ),
  );
}
