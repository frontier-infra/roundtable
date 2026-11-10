// Personas and Rounds: seat planning (same rules as the engines), the Seats and Rounds
// controls, the lab label, the lab pack switch, and custom persona CRUD in Settings.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, type MockOptions } from "../src/mock";
import { builtinPacks, libraryOf } from "../src/mock-personas";
import { labLabel, planSeats, placeSeats, findPersona, seatKeys, seatSpec } from "../src/state";

let root: HTMLElement;

beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
});

const tick = async (n = 8) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};

async function start(opts: MockOptions = {}, view: "main" | "settings" = "main") {
  const api = mockApi({ delay: 0, keys: ["claude", "gemini"], onboarded: true, ...opts });
  const app = new App(api, root);
  await app.start({ view });
  app.state.ask = { ...app.state.ask, coach: false };
  app.render();
  if (view === "settings") await app.openSettings();
  await tick();
  return { app, api };
}

const text = () => root.textContent ?? "";
const buttonNamed = (name: string, scope: ParentNode = root) => [...scope.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;
const click = async (name: string, scope: ParentNode = root) => {
  const b = buttonNamed(name, scope);
  if (!b) throw new Error(`no button "${name}" in: ${text()}`);
  b.click();
  await tick();
};
const choose = async (sel: HTMLSelectElement, value: string) => {
  sel.value = value;
  sel.dispatchEvent(new Event("change"));
  await tick();
};

describe("persona packs", () => {
  it("ship the four packs Jason asked for, with no person named on a lab seat", async () => {
    const lib = libraryOf(await builtinPacks(), true);
    expect(lib.packs.map((p) => p.id)).toEqual(["frontier-labs", "business-roles", "expert-panels"]);
    const roles = lib.packs[1].personas.map((p) => p.name);
    expect(roles).toEqual(["CEO", "CTO", "CFO", "COO", "CMO", "CISO", "General Counsel", "Head of Product", "Head of Sales", "Customer advocate", "Skeptical investor", "Board chair"]);
    const panels = lib.packs[2].presets.map((p) => [p.name, p.seats.length] as const);
    expect(panels.map((p) => p[0])).toEqual(["Security review", "Go-to-market", "Hiring decision", "Pricing", "Architecture review"]);
    for (const [, n] of panels) expect(n >= 3 && n <= 5).toBe(true);
    for (const p of lib.packs[0].personas) {
      expect(p.name).toMatch(/^The .+ view$/);
      expect(`${p.name} ${p.brief}`).not.toMatch(/\b(Jensen|Huang|Altman|Sam|Musk|Elon|Amodei|Dario|Hassabis|Demis|Nadella|Satya|Zuckerberg|Lisa Su)\b/);
      expect(p.lab).toBeTruthy();
    }
  });

  it("place seats like the engines: preference first, then the least-used AI", async () => {
    const lib = libraryOf(await builtinPacks(), true);
    const plan = planSeats({ seats: "preset:exec-team", seatList: [] }, ["claude", "gemini"], lib);
    expect(plan).toEqual([
      { persona: "ceo", head: "claude" },
      { persona: "cto", head: "gemini" },
      { persona: "cfo", head: "claude" },
      { persona: "coo", head: "gemini" },
    ]);
    expect(seatSpec(plan, ["claude", "gemini"])).toBe("ceo@claude,cto@gemini,cfo@claude,coo@gemini");
    // Generic is today's table, sent with no --personas at all.
    const generic = planSeats({ seats: "", seatList: [] }, ["claude", "gemini"], lib);
    expect(seatSpec(generic, ["claude", "gemini"])).toBe("");
    const cfo = { ...findPersona(lib, "CFO")!, ai: "gemini" };
    expect(placeSeats([findPersona(lib, "ceo")!, cfo], ["claude", "gemini"]).map((s) => s.head)).toEqual(["claude", "gemini"]);
    expect(seatKeys([{ persona: "cfo", head: "claude" }, { persona: "cfo", head: "claude" }, { persona: "", head: "gemini" }])).toEqual(["cfo@claude", "cfo@claude#2", "gemini"]);
  });

  it("hide the lab pack behind the switch and in a build without it", async () => {
    const packs = await builtinPacks();
    expect(libraryOf(packs, false).packs.map((p) => p.id)).toEqual(["business-roles", "expert-panels"]);
    expect(libraryOf(packs, true, false).packs.some((p) => p.labs)).toBe(false);
    expect(labLabel("NVIDIA")).toBe("Simulated perspective drawn from public positions. Not affiliated with or endorsed by NVIDIA.");
  });
});

describe("the ask box", () => {
  it("picks a preset, shows who sits where with the lab label, and sends the seats", async () => {
    const { app, api } = await start();
    const seats = root.querySelector<HTMLSelectElement>("select.seats-select")!;
    expect([...seats.options].map((o) => o.textContent)).toContain("Frontier four (4)");
    await choose(seats, "preset:frontier-four");
    expect(text()).toContain("4 seats on 2 AIs.");
    expect(text()).toContain("The NVIDIA view on Claude");
    expect(text()).toContain("Not affiliated with or endorsed by NVIDIA.");
    expect(text()).toContain("This will ask 2 AIs in 4 seats, 2 rounds, at most 10 requests.");
    const q = root.querySelector<HTMLTextAreaElement>("textarea.question")!;
    q.value = "Build or rent GPUs?";
    q.dispatchEvent(new Event("input"));
    await click("Ask the table");
    await tick(30);
    expect(api.calls).toContain("askStart:claude,gemini:2:nvidia-view@claude,openai-view@gemini,xai-view@claude,anthropic-view@gemini");
    const cards = [...root.querySelectorAll<HTMLElement>(".answer")];
    expect(cards.map((c) => c.querySelector("header strong")?.textContent)).toEqual(["The NVIDIA view", "The OpenAI view", "The xAI view", "The Anthropic view"]);
    expect(cards[0].querySelector(".who small")?.textContent).toBe("on Claude");
    expect(cards[0].querySelector(".lab-label")?.textContent).toBe("Simulated perspective drawn from public positions. Not affiliated with or endorsed by NVIDIA.");
    expect(app.state.settings.personas).toBe("preset:frontier-four");
  });

  it("runs 4 rounds and shows each one as a step", async () => {
    const { app, api } = await start();
    // Four rounds: Deep Think's count, without its coach (the CLI's --rounds 4).
    app.set({ ask: { ...app.state.ask, rounds: 4 } });
    await tick();
    expect(text()).toContain("This will ask 2 AIs, 4 rounds, at most 12 requests.");
    const q = root.querySelector<HTMLTextAreaElement>("textarea.question")!;
    q.value = "Q";
    q.dispatchEvent(new Event("input"));
    await click("Ask the table");
    await tick(40);
    expect(api.calls).toContain("askStart:claude,gemini:4");
    const steps = [...root.querySelectorAll<HTMLDetailsElement>(".step")];
    expect(steps).toHaveLength(4);
    expect(steps[3].querySelector(".state")?.textContent).toBe("Agreed");
    expect(steps[1].querySelector(".state")?.textContent).toBe("Still differ");
    expect(steps[2].textContent).toContain("(round 3)");
    expect(root.querySelector(".verdict .verdict-by")?.textContent).toBe("Claude, as chair, after round 4");
  });

  it("sets each seat by hand", async () => {
    const { api } = await start();
    await choose(root.querySelector<HTMLSelectElement>("select.seats-select")!, "custom");
    let rows = root.querySelectorAll(".seat-pick");
    expect(rows).toHaveLength(2);
    await choose(rows[0].querySelector<HTMLSelectElement>("select")!, "cfo");
    await choose(root.querySelectorAll(".seat-pick")[1].querySelector<HTMLSelectElement>("select")!, "ceo");
    await choose(root.querySelectorAll(".seat-pick")[1].querySelector<HTMLSelectElement>("select.ai-select")!, "claude");
    await click("Add a seat");
    rows = root.querySelectorAll(".seat-pick");
    expect(rows).toHaveLength(3);
    await choose(rows[2].querySelector<HTMLSelectElement>("select")!, "");
    await choose(root.querySelectorAll(".seat-pick")[2].querySelector<HTMLSelectElement>("select.ai-select")!, "gemini");
    const q = root.querySelector<HTMLTextAreaElement>("textarea.question")!;
    q.value = "Q";
    q.dispatchEvent(new Event("input"));
    await click("Ask the table");
    await tick(20);
    expect(api.calls).toContain("askStart:claude,gemini:2:cfo@claude,ceo@claude,generic@gemini");
    expect([...root.querySelectorAll(".answer")].map((c) => c.getAttribute("data-seat"))).toEqual(["cfo@claude", "ceo@claude", "gemini"]);
  });
});

describe("Settings, Personas", () => {
  it("creates, edits, duplicates and deletes a persona, and exports and imports packs", async () => {
    const { app, api } = await start({}, "settings");
    await click("New persona");
    const ed = root.querySelector<HTMLElement>(".persona-editor")!;
    const [name, expertise] = [...ed.querySelectorAll<HTMLInputElement>("input")];
    name.value = "Head of Support";
    name.dispatchEvent(new Event("input"));
    expertise.value = "Customers after they buy";
    expertise.dispatchEvent(new Event("input"));
    const brief = ed.querySelector("textarea")!;
    brief.value = "Weigh repeat contacts.";
    brief.dispatchEvent(new Event("input"));
    await choose(ed.querySelector<HTMLSelectElement>("select")!, "gemini");
    await click("Save", ed);
    expect(api.calls).toContain("personaSave:Head of Support");
    let row = root.querySelector<HTMLElement>('[data-persona="head-of-support"]')!;
    expect(row.textContent).toContain("Prefers Gemini");
    // Edit it.
    await click("Edit", row);
    const ed2 = root.querySelector<HTMLElement>(".persona-editor")!;
    const n2 = ed2.querySelector<HTMLInputElement>("input")!;
    n2.value = "Head of Support (EMEA)";
    n2.dispatchEvent(new Event("input"));
    await click("Save", ed2);
    row = root.querySelector<HTMLElement>('[data-persona="head-of-support"]')!;
    expect(row.querySelector("strong")?.textContent).toBe("Head of Support (EMEA)");
    // Duplicate a built-in one: it lands in your personas, open to edit.
    await click("Duplicate", root.querySelector<HTMLElement>('[data-pack="business-roles"] [data-persona="cfo"]')!);
    expect(app.state.personaEdit?.name).toBe("CFO (copy)");
    expect(root.querySelector('[data-persona="cfo-copy"]')).not.toBeNull();
    // Delete.
    const realConfirm = window.confirm;
    window.confirm = () => true;
    await click("Delete", root.querySelector<HTMLElement>('[data-persona="cfo-copy"]')!);
    window.confirm = realConfirm;
    expect(root.querySelector('[data-persona="cfo-copy"]')).toBeNull();
    await click("Export yours");
    await click("Import a pack");
    expect(api.calls).toEqual(expect.arrayContaining(["personasExport", "personasImport", "personaDelete:cfo-copy"]));
    expect(root.querySelector('[data-persona="ops-lead"]')).not.toBeNull();
  });

  it("the switch hides the lab pack everywhere", async () => {
    const { app } = await start({}, "settings");
    expect(root.querySelector('[data-pack="frontier-labs"]')).not.toBeNull();
    const sw = [...root.querySelectorAll<HTMLInputElement>("input.switch")].find((i) => i.closest("label")?.textContent?.includes("Frontier labs"))!;
    sw.checked = false;
    sw.dispatchEvent(new Event("change"));
    await tick(12);
    expect(app.state.settings.show_lab_pack).toBe(false);
    expect(root.querySelector('[data-pack="frontier-labs"]')).toBeNull();
    app.set({ view: "main" });
    const opts = [...root.querySelectorAll<HTMLOptionElement>("select.seats-select option")].map((o) => o.textContent ?? "");
    expect(opts.some((o) => o.includes("Frontier"))).toBe(false);
  });

  it("a build without the lab pack has no switch", async () => {
    await start({ noLabPack: true }, "settings");
    expect(text()).not.toContain("Show the Frontier labs pack");
    expect(root.querySelector('[data-pack="frontier-labs"]')).toBeNull();
  });
});
