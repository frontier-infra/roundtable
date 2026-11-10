// Credits and the Get Jebadiah hint: the links, the export line, the size picked by memory,
// and never a bare-name search hint.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, type MockOptions } from "../src/mock";
import ids from "../../engine/src/decision/jebadiah-ids.json";
import { EXPORT_CREDIT, JASON_URL, MADE_BY, TITANIUM_URL, withCredit } from "../src/credits";
import {
  appOrder,
  BUILDS,
  isOurModel,
  lmStudioId,
  OUR_HOSTS,
  OUR_NAMESPACES,
  modelPage,
  ollamaCommand,
  resetJebadiahView,
  sizeFor,
} from "../src/jebadiah";

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
  resetJebadiahView();
});
const tick = async (n = 10) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};
async function start(opts: MockOptions = {}, at?: { screen?: number; view?: "main" | "settings" }) {
  const api = mockApi({ delay: 0, ...opts });
  const app = new App(api, root);
  await app.start(at ?? {});
  await tick();
  return { app, api };
}
const text = () => root.textContent ?? "";
const linkNamed = (name: string) => [...root.querySelectorAll("a")].find((a) => a.textContent?.trim() === name) as HTMLAnchorElement | undefined;
const buttonNamed = (name: string) => [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;
const pres = () => [...root.querySelectorAll("pre")].map((p) => p.textContent ?? "");

describe("credits", () => {
  it("About in Settings says who made it and links Titanium and Jason, in the default browser", async () => {
    const { api } = await start({ keys: ["claude"], onboarded: true }, { view: "settings" });
    expect(text()).toContain(MADE_BY);
    expect(text()).toContain("Need AI in your business?");
    const t = linkNamed("Titanium Computing")!;
    expect(t.getAttribute("href")).toBe("https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=app&utm_campaign=about");
    t.click();
    expect(api.calls).toContain(`open:${TITANIUM_URL}`);
    linkNamed("Built by Jason Brashear, CTO")!.click();
    expect(api.calls).toContain(`open:${JASON_URL}`);
    expect(JASON_URL).toBe("https://titaniumcomputing.com/jason-brashear/?utm_source=roundtable&utm_medium=app");
  });

  it("the main screen has a one-line footer with the Titanium link", async () => {
    await start({ keys: ["claude"], onboarded: true });
    const f = root.querySelector(".credit-footer")!;
    expect(f.textContent).toBe("Free from Titanium Computing");
    expect(f.querySelector("a")!.getAttribute("href")).toBe(TITANIUM_URL);
  });

  it("the last setup screen says Free from Titanium Computing, and earlier screens do not", async () => {
    await start({ keys: ["claude"] }, { screen: 4 });
    expect(linkNamed("Free from Titanium Computing")).toBeUndefined();
    await start({ keys: ["claude"] }, { screen: 5 });
    expect(linkNamed("Free from Titanium Computing")!.getAttribute("href")).toBe(TITANIUM_URL);
  });

  it("exports end with one credit line, once, and the setting turns it off", () => {
    const md = "# Round Table\n\nThe verdict.\n";
    const out = withCredit(md, true);
    expect(out.endsWith(`${EXPORT_CREDIT}\n`)).toBe(true);
    expect(EXPORT_CREDIT).toBe(
      "Made with Roundtable (https://roundtable.sh), free from Titanium Computing (https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=export)",
    );
    expect(withCredit(out, true)).toBe(out);
    expect(withCredit(md, undefined)).toContain(EXPORT_CREDIT);
    expect(withCredit(md, false)).toBe(md);
  });

  it("Download .md on a past roundtable carries the line unless it is off", async () => {
    const history = [{ id: "100-abcd", ts: 100, question: "From Claude", source: "claude", mode: "advisory", heads: ["claude"] }];
    const { app, api } = await start({ keys: ["claude"], onboarded: true, history });
    await app.openHistory("100-abcd");
    await tick();
    buttonNamed("Download .md")!.click();
    await tick();
    expect(api.calls.find((c) => c.startsWith("save:"))).toContain(EXPORT_CREDIT);
    await app.saveSettings({ export_credit: false });
    api.calls.length = 0;
    buttonNamed("Download .md")!.click();
    await tick();
    const saved = api.calls.find((c) => c.startsWith("save:"))!;
    expect(saved).toContain("A saved answer.");
    expect(saved).not.toContain(EXPORT_CREDIT);
  });

  it("the verdict card's Copy as Markdown and Download .md, and Export session, end with the line once", async () => {
    const { app, api } = await start({ keys: ["claude", "gemini"], onboarded: true });
    app.state.ask = { ...app.state.ask, coach: false, verdict: "decision" };
    await app.askTable("Bread or garden?", null);
    await tick(30);
    const card = root.querySelector(".final-verdict")!;
    const cardButton = (name: string) => [...card.querySelectorAll("button")].find((b) => b.textContent === name) as HTMLButtonElement;
    cardButton("Download .md").click();
    await tick();
    const saved = api.calls.find((c) => c.startsWith("save:"))!;
    expect(saved.endsWith(`${EXPORT_CREDIT}\n`)).toBe(true);
    expect(saved.split(EXPORT_CREDIT).length).toBe(2);
    const body = saved.slice(saved.indexOf(":", 5) + 1);
    api.calls.length = 0;
    cardButton("Copy as Markdown").click();
    await tick();
    expect(api.calls).toContain(`copy:${body.length}`);
    expect(app.state.toast).toBe("Copied as Markdown.");
    await app.exportSession();
    expect(api.calls.find((c) => c.startsWith("save:"))!.endsWith(`${EXPORT_CREDIT}\n`)).toBe(true);
    await app.saveSettings({ export_credit: false });
    api.calls.length = 0;
    await app.downloadMarkdown();
    expect(api.calls.find((c) => c.startsWith("save:"))).not.toContain(EXPORT_CREDIT);
  });
});

describe("Get Jebadiah", () => {
  it("picks the size by memory: 4B under 16 GB, 9B up to 32 GB, 27B above", () => {
    expect(sizeFor(8)).toBe("4b");
    expect(sizeFor(15)).toBe("4b");
    expect(sizeFor(15.8)).toBe("9b"); // a 16 GB Windows PC reports a little less
    expect(sizeFor(16)).toBe("9b");
    expect(sizeFor(32)).toBe("9b");
    expect(sizeFor(36)).toBe("27b");
    expect(sizeFor(256)).toBe("27b");
    expect(sizeFor(null)).toBe("9b");
  });

  it("every command, id and page uses the full frontier-infra address, and Ollama the library name", () => {
    for (const size of ["4b", "9b", "27b"] as const) {
      expect(ollamaCommand(size)).toBe(`ollama pull frontierinfra/jebadiah:${size}`);
      expect(lmStudioId(size, true)).toBe(`frontier-infra/${BUILDS[size].mlx}`);
      expect(lmStudioId(size, false)).toBe(`frontier-infra/${BUILDS[size].gguf}`);
      expect(modelPage(size, true)).toBe(`https://huggingface.co/frontier-infra/${BUILDS[size].mlx}`);
    }
    expect(ollamaCommand("9b")).toBe("ollama pull frontierinfra/jebadiah:9b");
  });

  it("offers LM Studio with MLX first on Apple Silicon, Ollama first elsewhere", () => {
    expect(appOrder("macos", "aarch64")).toEqual(["lmstudio", "ollama"]);
    expect(appOrder("macos", "x86_64")).toEqual(["ollama", "lmstudio"]);
    expect(appOrder("windows", "x86_64")).toEqual(["ollama", "lmstudio"]);
  });

  it("knows our builds and not the other account's models", () => {
    expect(isOurModel("hf.co/frontier-infra/jebadiah-9b-v2-1-GGUF:Q4_K_M")).toBe(true);
    expect(isOurModel("frontierinfra/jebadiah:27b")).toBe(true);
    expect(isOurModel("jbrashearfrontierinfra/jebadiah:latest")).toBe(true);
    expect(isOurModel("Jebadiah/Aria-7b-128k-v1")).toBe(false);
    expect(isOurModel("jebadiah/hermes-lexi-fun-stone-l3-8b")).toBe(false);
    // A stranger's account named Jebadiah, and a bare name anyone could publish.
    expect(isOurModel("hf.co/Jebadiah/jebadiah-9b-uncensored-GGUF:Q4_K_M")).toBe(false);
    expect(isOurModel("jebadiah-9b-v2-1-mlx-8bit")).toBe(false);
  });

  it("uses the engine's one list of our id patterns", () => {
    expect(OUR_NAMESPACES).toEqual(ids.namespaces);
    expect(OUR_HOSTS).toEqual(ids.hosts);
    for (const id of ids.ours) expect(isOurModel(id), id).toBe(true);
    for (const id of ids.not_ours) expect(isOurModel(id), id).toBe(false);
  });

  it("Settings shows the hint on an Apple Silicon Mac with 64 GB: LM Studio and the 27B MLX build first", async () => {
    const { api } = await start({ keys: ["claude"], onboarded: true, memoryGb: 64, arch: "aarch64" }, { view: "settings" });
    await tick();
    expect(api.calls).toContain("jebadiahLocal");
    expect(text()).toContain("Get Jebadiah, our free decision model");
    // Inside the Decision model section, not a section of its own.
    const sections = [...root.querySelectorAll("section.setting")].filter((s) => s.querySelector("h2")?.textContent === "Decision model");
    expect(sections.length).toBe(1);
    expect(sections[0].textContent).toContain("Get Jebadiah, our free decision model");
    expect(sections[0].querySelector('[role="radiogroup"][aria-label="Decision model"]')).not.toBeNull();
    expect(root.querySelector(".choice.on")!.getAttribute("data-jeb-app")).toBe("lmstudio");
    expect(pres()).toContain("frontier-infra/jebadiah-27b-v2-1-MLX-4bit");
    expect(text()).toContain("picked for 64 GB of memory");
    expect(linkNamed("Its page on Hugging Face")!.getAttribute("href")).toBe("https://huggingface.co/frontier-infra/jebadiah-27b-v2-1-MLX-4bit");
    (root.querySelector('[data-jeb-app="ollama"]') as HTMLButtonElement).click();
    expect(pres()).toContain("ollama pull frontierinfra/jebadiah:27b");
    const cmd = "ollama pull frontierinfra/jebadiah:27b";
    const box = [...root.querySelectorAll(".copy-box")].find((b) => b.querySelector("pre")?.textContent === cmd)!;
    (box.querySelector("button") as HTMLButtonElement).click();
    await tick();
    expect(api.calls).toContain(`copy:${cmd.length}`);
  });

  it("a Windows PC with 16 GB gets Ollama first and the 9B", async () => {
    await start({ keys: ["claude"], onboarded: true, os: "windows", arch: "x86_64", memoryGb: 15.8 }, { view: "settings" });
    await tick();
    expect(root.querySelector(".choice.on")!.getAttribute("data-jeb-app")).toBe("ollama");
    expect(pres()).toContain("ollama pull frontierinfra/jebadiah:9b");
    expect(text()).toContain("paste this into PowerShell");
  });

  it("no hint ever shows the bare name as something to search for or pull", async () => {
    for (const opts of [{ memoryGb: 8 }, { memoryGb: 24, arch: "x86_64" }, { memoryGb: 128 }]) {
      resetJebadiahView();
      document.body.innerHTML = '<div id="app"></div>';
      root = document.getElementById("app")!;
      await start({ keys: ["claude"], onboarded: true, ...opts }, { view: "settings" });
      await tick();
      for (const p of pres()) expect(p).toMatch(/frontier-infra\/|frontierinfra\/jebadiah:/);
      for (const btn of root.querySelectorAll("[data-jeb-app]")) {
        (btn as HTMLButtonElement).click();
        for (const p of pres()) expect(p).toMatch(/frontier-infra\/|frontierinfra\/jebadiah:/);
      }
    }
  });

  it("after install, Check again finds it", async () => {
    const api = mockApi({ delay: 0, keys: ["claude"], onboarded: true });
    const app = new App(api, root);
    await app.start({ view: "settings" });
    await tick();
    expect(text()).toContain("Get Jebadiah, our free decision model");
    api.jebadiahLocal = async () => ({ os: "macos", arch: "aarch64", memory_gb: 32, found: { app: "Ollama", model: "hf.co/frontier-infra/jebadiah-9b-v2-1-GGUF:Q4_K_M" }, served: null, hint: null, looked: [] });
    buttonNamed("Check again")!.click();
    await tick();
    expect(text()).toContain("Jebadiah is installed in Ollama: hf.co/frontier-infra/jebadiah-9b-v2-1-GGUF:Q4_K_M.");
    expect(text()).not.toContain("Get Jebadiah, our free decision model");
  });

  it("when decision detect finds a Jebadiah server, the hint does not show", async () => {
    await start({ keys: ["claude"], onboarded: true, jebadiahServed: { url: "http://127.0.0.1:8100", model: "jebadiah-9b-v2-1", via: "jeb serve" } }, { view: "settings" });
    await tick();
    expect(text()).toContain("Found Jebadiah (jeb serve) at http://127.0.0.1:8100: jebadiah-9b-v2-1.");
    expect(text()).not.toContain("Get Jebadiah, our free decision model");
  });

  it("the hint shows with the decision model off or on Jebadiah, not with Jev or another model", async () => {
    const { app } = await start({ keys: ["claude"], onboarded: true }, { view: "settings" });
    await tick();
    expect(text()).toContain("Get Jebadiah, our free decision model");
    await app.saveSettings({ decision: { provider: "jebadiah", url: null, model: null, name: null } });
    await tick();
    expect(text()).toContain("Get Jebadiah, our free decision model");
    await app.saveSettings({ decision: { provider: "jev", url: null, model: null, name: null } });
    await tick();
    expect(text()).not.toContain("Get Jebadiah, our free decision model");
  });
});
