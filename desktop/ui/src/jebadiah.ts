// "Get Jebadiah, our free decision model": the install hint shown inside the Decision model
// section of Settings when `roundtable decision detect` finds no Jebadiah server and none of
// our builds is in Ollama or LM Studio.
//
// Every hint names the full frontier-infra address. A different Hugging Face account with a
// similar name holds unrelated models, so a bare-name search would find the wrong thing.

import type { App } from "./app";
import type { JebadiahLocal } from "./api";
import { button, h } from "./dom";
import { outLink } from "./credits";

export type JebSize = "4b" | "9b" | "27b";
export type JebApp = "ollama" | "lmstudio";

export const HF_ORG = "frontier-infra";
export const OLLAMA_DOWNLOAD_URL = "https://ollama.com/download";
export const LMSTUDIO_DOWNLOAD_URL = "https://lmstudio.ai";

/** On since 2026-11-10: frontierinfra/jebadiah is live in the Ollama library (4b, 9b, 27b, latest = 9b). */
export const OLLAMA_LIBRARY_LIVE = true;
export const OLLAMA_LIBRARY = "frontierinfra/jebadiah";

interface Build {
  label: string;
  /** GGUF repo under frontier-infra, for Ollama, LM Studio on Windows and Intel Macs. */
  gguf: string;
  /** MLX repo under frontier-infra, for LM Studio on Apple Silicon. */
  mlx: string;
}

export const BUILDS: Record<JebSize, Build> = {
  "4b": { label: "4B", gguf: "jebadiah-4b-v2-GGUF", mlx: "jebadiah-4b-v2-MLX-8bit" },
  "9b": { label: "9B", gguf: "jebadiah-9b-v2-1-GGUF", mlx: "jebadiah-9b-v2-1-MLX-8bit" },
  "27b": { label: "27B", gguf: "jebadiah-27b-v2-1-GGUF", mlx: "jebadiah-27b-v2-1-MLX-4bit" },
};

/** The GGUF quant Ollama pulls. */
export const OLLAMA_QUANT = "Q4_K_M";

/** 4B under 16 GB, 9B up to 32 GB, 27B above. Unknown memory gets the 9B. */
export function sizeFor(memoryGb: number | null | undefined): JebSize {
  if (memoryGb === null || memoryGb === undefined || !Number.isFinite(memoryGb) || memoryGb <= 0) return "9b";
  const gb = Math.round(memoryGb);
  if (gb < 16) return "4b";
  if (gb <= 32) return "9b";
  return "27b";
}

export function isAppleSilicon(os: string, arch: string): boolean {
  return os === "macos" && (arch === "aarch64" || arch === "arm64");
}

/** The order the two apps are offered in: LM Studio with MLX first on Apple Silicon. */
export function appOrder(os: string, arch: string): JebApp[] {
  return isAppleSilicon(os, arch) ? ["lmstudio", "ollama"] : ["ollama", "lmstudio"];
}

export function ollamaCommand(size: JebSize): string {
  return OLLAMA_LIBRARY_LIVE ? `ollama pull ${OLLAMA_LIBRARY}:${size}` : `ollama pull hf.co/${HF_ORG}/${BUILDS[size].gguf}:${OLLAMA_QUANT}`;
}

/** The exact id to paste into LM Studio's search. */
export function lmStudioId(size: JebSize, appleSilicon: boolean): string {
  return `${HF_ORG}/${appleSilicon ? BUILDS[size].mlx : BUILDS[size].gguf}`;
}

export function modelPage(size: JebSize, appleSilicon: boolean): string {
  return `https://huggingface.co/${lmStudioId(size, appleSilicon)}`;
}

/** Namespaces that publish our Jebadiah. Same list as engine/src/decision/jebadiah-ids.json. */
export const OUR_NAMESPACES = ["frontier-infra", "frontierinfra", "jbrashearfrontierinfra"];
/** Hosts dropped before the namespace is read. */
export const OUR_HOSTS = ["hf.co/", "huggingface.co/", "registry.ollama.ai/"];

/** True for our builds only, never for the other account's models (the engine's `is_ours`). */
export function isOurModel(id: string): boolean {
  let low = id.trim().toLowerCase().split(":")[0];
  const host = OUR_HOSTS.find((h) => low.startsWith(h));
  if (host) low = low.slice(host.length);
  const slash = low.indexOf("/");
  if (slash < 0) return false;
  const ns = low.slice(0, slash);
  const name = low.slice(slash + 1);
  return OUR_NAMESPACES.includes(ns) && (name === "jebadiah" || name.startsWith("jebadiah-"));
}

// ---- The view ----

interface ViewState {
  local: JebadiahLocal | null;
  loading: boolean;
  error: string | null;
  pick: JebApp | null;
}

const view: ViewState = { local: null, loading: false, error: null, pick: null };

/** For tests. */
export function resetJebadiahView() {
  view.local = null;
  view.loading = false;
  view.error = null;
  view.pick = null;
}

async function check(app: App) {
  view.loading = true;
  view.error = null;
  app.render();
  try {
    view.local = await app.api.jebadiahLocal();
  } catch {
    view.error = "Could not look on this computer. Try again.";
  }
  view.loading = false;
  app.render();
}

function copyBox(app: App, text: string): HTMLElement {
  return h(
    "div",
    { class: "copy-box" },
    h("pre", { text }),
    button("Copy", () => void app.api.copyText(text).then(() => app.showToast("Copied.")), "btn small"),
  );
}

function memoryLine(local: JebadiahLocal, size: JebSize): string {
  const gb = local.memory_gb ? Math.round(local.memory_gb) : null;
  return gb ? `The ${BUILDS[size].label} build, picked for ${gb} GB of memory.` : `The ${BUILDS[size].label} build.`;
}

/** The local-Jebadiah part of the Decision model section: what detection found, or how to get it. */
export function getJebadiah(app: App): HTMLElement[] {
  if (!view.local && !view.loading && !view.error) {
    void check(app);
  }
  const again = () => button(view.loading ? "Looking..." : "Check again", () => void check(app), "btn", view.loading);
  const local = view.local;
  if (!local) {
    const out: HTMLElement[] = [h("p", { class: "muted", text: view.error ?? "Looking on this computer..." })];
    if (view.error) out.push(h("div", { class: "actions left" }, again()));
    return out;
  }
  if (local.served) {
    return [
      h("p", { class: "status ok", text: `Found Jebadiah (${local.served.via}) at ${local.served.url}: ${local.served.model}.` }),
      h("div", { class: "actions left" }, again()),
    ];
  }
  if (local.found) {
    const out: HTMLElement[] = [h("p", { class: "status ok", text: `Jebadiah is installed in ${local.found.app}: ${local.found.model}.` })];
    if (local.hint) out.push(h("p", { class: "small muted", text: local.hint }));
    out.push(h("div", { class: "actions left" }, again()));
    return out;
  }
  const apple = isAppleSilicon(local.os, local.arch);
  const order = appOrder(local.os, local.arch);
  const pick = view.pick ?? order[0];
  const size = sizeFor(local.memory_gb);
  const names: Record<JebApp, { title: string; hint: string }> = {
    ollama: { title: "Ollama", hint: "One command in Terminal." },
    lmstudio: { title: "LM Studio", hint: apple ? "The MLX build, fastest on Apple Silicon." : "Paste one model id." },
  };
  const choices = h(
    "div",
    { class: "choices", role: "radiogroup", attrs: { "aria-label": "Where to run Jebadiah" } },
    order.map((id) =>
      h(
        "button",
        {
          class: `choice${id === pick ? " on" : ""}`,
          type: "button",
          role: "radio",
          attrs: { "aria-checked": String(id === pick), "data-jeb-app": id },
          on: {
            click: () => {
              view.pick = id;
              app.render();
            },
          },
        },
        h("strong", { text: names[id].title }),
        h("span", { text: names[id].hint }),
      ),
    ),
  );
  const shell = local.os === "windows" ? "PowerShell" : "Terminal";
  const detail =
    pick === "ollama"
      ? [
          h("p", { class: "small", text: `With Ollama running, paste this into ${shell}:` }),
          copyBox(app, ollamaCommand(size)),
          h("p", { class: "small muted" }, memoryLine(local, size), " No Ollama yet? ", outLink(app, OLLAMA_DOWNLOAD_URL, "Get Ollama")),
        ]
      : [
          h("p", { class: "small", text: "In LM Studio, search for this exact model id and download it:" }),
          copyBox(app, lmStudioId(size, apple)),
          h(
            "p",
            { class: "small muted" },
            memoryLine(local, size),
            " ",
            outLink(app, modelPage(size, apple), "Its page on Hugging Face"),
            ". No LM Studio yet? ",
            outLink(app, LMSTUDIO_DOWNLOAD_URL, "Get LM Studio"),
          ),
        ];
  return [
    h("h3", { text: "Get Jebadiah, our free decision model" }),
    h("p", { class: "small muted", text: "Jebadiah rates the table's verdict on this computer. It is free and nothing leaves your machine." }),
    choices,
    ...detail,
    h("p", { class: "small muted", text: "Use the full frontier-infra id. Another account with a similar name holds unrelated models." }),
    h("div", { class: "actions left" }, again()),
  ];
}
