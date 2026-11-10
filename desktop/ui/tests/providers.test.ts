// Providers the person adds: OpenRouter, Ollama, LM Studio, Groq, Together, Custom.
// Every ticked model is its own seat (`provider:model`), at the table, in the cost line,
// with personas, Test and Retry.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, type MockOptions } from "../src/mock";
import type { ProviderConfig, ProviderRow } from "../src/api";
import { connectedHeads, costLine, emptyKeys, placeSeats, priceText, providerSeatsOn, seatCost, seatKeys, seatSpec, initialState } from "../src/state";
import { family, providerName, setProviderNames, splitSeat } from "../src/providers";

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
  setProviderNames([]);
});
const tick = async (n = 12) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};
async function start(opts: MockOptions = {}, view: "main" | "settings" = "settings") {
  const api = mockApi({ delay: 0, onboarded: true, ...opts });
  const app = new App(api, root);
  await app.start({ view: "main" });
  app.state.ask = { ...app.state.ask, coach: false };
  if (view === "settings") await app.openSettings();
  await tick();
  return { app, api };
}
const text = () => root.textContent ?? "";
const buttonNamed = (name: string, scope: ParentNode = root) =>
  [...scope.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;
const card = (id: string) => root.querySelector(`.provider-card[data-provider="${id}"]`) as HTMLElement;
const box = (scope: ParentNode, model: string) => scope.querySelector(`.model-row[data-model="${model}"] input`) as HTMLInputElement;

const OPENROUTER: ProviderConfig = {
  id: "openrouter",
  preset: "openrouter",
  name: "OpenRouter",
  base_url: "https://openrouter.ai/api/v1",
  needs_key: true,
  models: [{ id: "openai/gpt-4o-mini", price_in: 0.15, price_out: 0.6 }],
};

describe("seat ids and names", () => {
  it("splits at the first colon and names model and provider", () => {
    expect(splitSeat("ollama:llama3.2:3b")).toEqual({ provider: "ollama", model: "llama3.2:3b" });
    expect(splitSeat("openrouter:meta-llama/llama-3.3-70b-instruct:free")?.model).toBe("meta-llama/llama-3.3-70b-instruct:free");
    expect(splitSeat("claude")).toBeNull();
    expect(splitSeat("cfo@claude")).toBeNull();
    setProviderNames([{ id: "my-proxy", name: "My proxy" }]);
    expect(providerName("my-proxy:small-a")).toBe("small-a (My proxy)");
    expect(providerName("ollama:qwen3:4b")).toBe("qwen3:4b (Ollama)");
    expect(providerName("claude")).toBe("Claude");
    expect(family("ollama:qwen3:4b")).toBe("ollama");
    expect(family("claude")).toBe("claude");
  });

  it("puts provider seats after the built-in AIs", () => {
    const keys = emptyKeys();
    keys.claude = { status: "connected" };
    expect(connectedHeads(keys, ["codex", "ollama:qwen3:4b", "openrouter:openai/gpt-4o-mini"])).toEqual(["claude", "codex", "ollama:qwen3:4b", "openrouter:openai/gpt-4o-mini"]);
  });

  it("counts only seats a run can ask once the list is in", () => {
    const s = initialState();
    s.settings.providers = [OPENROUTER];
    expect(providerSeatsOn(s)).toEqual(["openrouter:openai/gpt-4o-mini"]);
    const row: ProviderRow = { ...OPENROUTER, has_key: false, key_source: null, local: false, note: null, key_page: "", ready: false, models: [{ ...OPENROUTER.models[0], seat: "openrouter:openai/gpt-4o-mini" }] };
    s.providers = { ...s.providers, loaded: true, list: [row] };
    expect(providerSeatsOn(s)).toEqual([]);
    s.providers.list = [{ ...row, has_key: true, ready: true }];
    expect(providerSeatsOn(s)).toEqual(["openrouter:openai/gpt-4o-mini"]);
  });
});

describe("the suggested panel mixes providers", () => {
  it("puts the next persona on a different provider when seats tie", () => {
    const p = (id: string) => ({ id, name: id, expertise: "", brief: "" });
    const heads = ["openrouter:a", "openrouter:b", "ollama:c"];
    const seats = placeSeats([p("cfo"), p("ceo"), p("cto")], heads);
    expect(seats.map((s) => family(s.head))).toEqual(["openrouter", "ollama", "openrouter"]);
    // Built-in AIs place as before.
    expect(placeSeats([p("cfo"), p("ceo"), p("cto")], ["claude", "gemini"]).map((s) => s.head)).toEqual(["claude", "gemini", "claude"]);
    // The seat spec and keys carry the seat id after the persona's @.
    expect(seatSpec(seats, heads)).toBe("cfo@openrouter:a,ceo@ollama:c,cto@openrouter:b");
    expect(seatKeys(seats)[1]).toBe("ceo@ollama:c");
  });
});

describe("the cost line", () => {
  const rows = (): ProviderRow[] => [
    { ...OPENROUTER, has_key: true, key_source: "keychain", local: false, note: null, key_page: "", ready: true, models: [{ ...OPENROUTER.models[0], seat: "openrouter:openai/gpt-4o-mini" }, { id: "openrouter/auto", seat: "openrouter:openrouter/auto" }] },
    { id: "ollama", preset: "ollama", name: "Ollama", base_url: "http://127.0.0.1:11434/v1", needs_key: false, has_key: false, key_source: null, local: true, note: "free, on this computer", key_page: "", ready: true, models: [{ id: "qwen3:4b", seat: "ollama:qwen3:4b" }] },
  ];

  it("says nothing without a provider seat", () => {
    expect(costLine(["claude", "gemini"], 2, rows())).toBeNull();
  });

  it("uses published prices, says free on this computer, else unknown", () => {
    setProviderNames(rows());
    const line = costLine(["claude", "openrouter:openai/gpt-4o-mini", "openrouter:openrouter/auto", "ollama:qwen3:4b"], 1, rows())!;
    expect(line).toBe("Cost: under $0.01 at published prices; 1 seat free, on this computer; unknown for Claude, openrouter/auto (OpenRouter).");
    // 3 rounds of a pricey model: later rounds read every seat.
    const pricey = rows();
    pricey[0].models[0] = { ...pricey[0].models[0], price_in: 15, price_out: 75 };
    const r3 = costLine(["openrouter:openai/gpt-4o-mini", "ollama:qwen3:4b"], 3, pricey)!;
    expect(r3).toBe(`Cost: about $${seatCost(15, 75, 2, 3).toFixed(2)} at published prices; 1 seat free, on this computer.`);
    expect(seatCost(15, 75, 2, 3)).toBeGreaterThan(seatCost(15, 75, 2, 1) * 3);
  });

  it("prices a model in plain words", () => {
    expect(priceText({ free: true })).toBe("free, on this computer");
    expect(priceText({ price_in: 0.15, price_out: 0.6 })).toBe("$0.15 in, $0.6 out per million tokens");
    expect(priceText({ price_in: 3, price_out: 15 })).toBe("$3 in, $15 out per million tokens");
    expect(priceText({})).toBe("price unknown");
  });
});

describe("Settings, AI accounts, Other providers", () => {
  it("finds Ollama on this computer and seats a ticked model", async () => {
    const { app, api } = await start({ keys: ["claude"], localProviders: "ollama" });
    const c = card("ollama");
    expect(c).toBeTruthy();
    expect(c.textContent).toContain("free, on this computer");
    expect(c.textContent).toContain("found on this computer");
    box(c, "qwen3:4b").click();
    await tick(20);
    expect(api.calls).toContain("providerAdd:ollama:qwen3:4b");
    expect(api.calls).toContain("headTest:ollama:qwen3:4b:auto");
    // Now it is an added provider with one seat and a Test button.
    const added = card("ollama");
    expect(added.getAttribute("data-found")).toBeNull();
    const seat = added.querySelector('.seat-item[data-seat="ollama:qwen3:4b"]')!;
    expect(seat.textContent).toContain("free, on this computer");
    expect(buttonNamed("Test", seat)).toBeTruthy();
    expect(app.cliHeads()).toContain("ollama:qwen3:4b");
    // It sits at the table and in "Who sits at the table".
    expect(text()).toContain("qwen3:4b (Ollama)");
  });

  it("adds OpenRouter with a key, refuses a bad one, searches and ticks models", async () => {
    const { app, api } = await start({ keys: ["claude"] });
    buttonNamed("Add a provider")!.click();
    await tick();
    const form = root.querySelector(".provider-form")!;
    expect(form.textContent).toContain("Where do I get a key?");
    const key = form.querySelector<HTMLInputElement>('input[aria-label="Provider key"]')!;
    expect(key.type).toBe("password");
    key.value = "sk-or-bad";
    key.dispatchEvent(new Event("input"));
    buttonNamed("Connect", form)!.click();
    await tick(20);
    expect(root.querySelector(".provider-form .status.bad")!.textContent).toContain("needs a working key");
    // The refused key is cleared from the field.
    expect(root.querySelector<HTMLInputElement>('input[aria-label="Provider key"]')!.value).toBe("");
    const k2 = root.querySelector<HTMLInputElement>('input[aria-label="Provider key"]')!;
    k2.value = "sk-or-good";
    k2.dispatchEvent(new Event("input"));
    buttonNamed("Connect", root.querySelector(".provider-form")!)!.click();
    await tick(30);
    expect(api.calls).toContain("providerAdd:openrouter:key");
    expect(api.calls).toContain("providerModels:openrouter");
    const c = card("openrouter");
    expect(c.textContent).toContain("Key saved in your Mac's Keychain.");
    // Prices shown, a stranger's Jebadiah is marked not ours.
    expect(c.querySelector('.model-row[data-model="openai/gpt-4o-mini"]')!.textContent).toContain("$0.15 in, $0.6 out per million tokens");
    expect(c.querySelector('.model-row[data-model="openrouter/auto"]')!.textContent).toContain("price unknown");
    expect(c.querySelector('.model-row[data-model="someone/jebadiah-7b"]')!.textContent).toContain("Not a frontier-infra build");
    // Search hides rows in place and keeps the box.
    const search = c.querySelector<HTMLInputElement>(".model-search")!;
    search.focus();
    search.value = "LLAMA 70b";
    search.dispatchEvent(new Event("input"));
    const visible = [...c.querySelectorAll<HTMLElement>(".model-row")].filter((r) => !r.hidden).map((r) => r.dataset.model);
    expect(visible).toEqual(["meta-llama/llama-3.3-70b-instruct"]);
    expect(document.activeElement).toBe(search);
    // Tick two: two seats.
    box(c, "meta-llama/llama-3.3-70b-instruct").click();
    await tick(20);
    box(card("openrouter"), "openai/gpt-4o-mini").click();
    await tick(20);
    expect(api.calls).toContain("providerAdd:openrouter:meta-llama/llama-3.3-70b-instruct,openai/gpt-4o-mini");
    expect(app.cliHeads()).toEqual(["openrouter:meta-llama/llama-3.3-70b-instruct", "openrouter:openai/gpt-4o-mini"]);
    // The search survives the redraw.
    expect(card("openrouter").querySelector<HTMLInputElement>(".model-search")!.value).toBe("LLAMA 70b");
  });

  it("adds a Custom provider by address and removes it with its seats", async () => {
    const { app, api } = await start({ keys: ["claude"] });
    buttonNamed("Add a provider")!.click();
    await tick();
    const sel = root.querySelector<HTMLSelectElement>('.provider-form select[aria-label="Provider"]')!;
    sel.value = "custom";
    sel.dispatchEvent(new Event("change"));
    await tick();
    const form = root.querySelector(".provider-form")!;
    expect(form.querySelector<HTMLInputElement>('input[aria-label="Provider key"]')!.placeholder).toBe("Only if it needs one");
    const name = form.querySelector<HTMLInputElement>('input[aria-label="Provider name"]')!;
    name.value = "Kraken proxy";
    name.dispatchEvent(new Event("input"));
    const url = form.querySelector<HTMLInputElement>('input[aria-label="Base URL"]')!;
    url.value = "https://proxy.example.com/v1";
    url.dispatchEvent(new Event("input"));
    buttonNamed("Connect", form)!.click();
    await tick(30);
    expect(api.calls).toContain("providerAdd:custom");
    box(card("kraken-proxy"), "small-a").click();
    await tick(20);
    expect(app.cliHeads()).toEqual(["kraken-proxy:small-a"]);
    const confirm = globalThis.confirm;
    globalThis.confirm = () => true;
    buttonNamed("Remove", card("kraken-proxy"))!.click();
    globalThis.confirm = confirm;
    await tick(20);
    expect(api.calls).toContain("providerRemove:kraken-proxy");
    expect(card("kraken-proxy")).toBeNull();
    expect(app.cliHeads()).toEqual([]);
  });

  it("a keyed provider without its key is not at the table and says so", async () => {
    const { app } = await start({ keys: ["claude"], providers: [OPENROUTER] });
    expect(card("openrouter").textContent).toContain("Needs a key.");
    expect(app.cliHeads()).toEqual([]);
    expect(buttonNamed("Add key", card("openrouter"))).toBeTruthy();
  });
});

describe("provider seats at the table", () => {
  it("shows the seats and the cost line, runs them, and Test and Retry work", async () => {
    const { app, api } = await start(
      { keys: ["claude"], providers: [OPENROUTER, { id: "ollama", preset: "ollama", name: "Ollama", base_url: "http://127.0.0.1:11434/v1", needs_key: false, models: [{ id: "qwen3:4b" }] }], providerKeys: ["openrouter"], runFails: { "ollama:qwen3:4b": { message: "Ollama is not answering on this computer. Is it running?", fix: "retry" } } },
      "main",
    );
    await tick(20);
    app.set({ view: "main" });
    await tick();
    const cost = root.querySelector("[data-cost]")!;
    expect(cost.textContent).toBe("Cost: under $0.01 at published prices; 1 seat free, on this computer; unknown for Claude.");
    expect(root.querySelector(".seat-row")!.getAttribute("aria-label")).toContain("gpt-4o-mini (OpenRouter)");
    await app.ask("Bread or garden?", "quick", false);
    await tick(40);
    expect(api.calls).toContain("askStart:claude,openrouter:openai/gpt-4o-mini,ollama:qwen3:4b:1");
    const or = root.querySelector('.answer[data-head="openrouter:openai/gpt-4o-mini"]')!;
    expect(or.textContent).toContain("gpt-4o-mini (OpenRouter)");
    expect(or.textContent).toContain("Answer from openai/gpt-4o-mini.");
    const ol = root.querySelector('.answer[data-head="ollama:qwen3:4b"]')!;
    expect(ol.textContent).toContain("Is it running?");
    buttonNamed("Retry", ol)!.click();
    await tick(20);
    expect(api.calls).toContain("askRetry:ollama:qwen3:4b");
    expect(app.state.run!.cards["ollama:qwen3:4b"].status).toBe("answered");
  });

  it("works with personas and rounds 1 to 4", async () => {
    const { app, api } = await start({ keys: ["claude"], providers: [OPENROUTER], providerKeys: ["openrouter"] }, "main");
    await tick(20);
    app.state.ask = { ...app.state.ask, coach: false, rounds: 3, seats: "custom", seatList: [{ persona: "cfo", head: "openrouter:openai/gpt-4o-mini" }, { persona: "ceo", head: "claude" }] };
    await app.ask("Raise prices?", 3, false);
    await tick(60);
    expect(api.calls).toContain("askStart:claude,openrouter:openai/gpt-4o-mini:3:cfo@openrouter:openai/gpt-4o-mini,ceo@claude");
    expect(app.state.run!.cards["cfo@openrouter:openai/gpt-4o-mini"].status).toBe("answered");
  });
});
