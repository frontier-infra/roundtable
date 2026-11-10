// Quick and Deep Think: the mode control in the ask box, the coach's Deep Think suggestion,
// the verdict card, and the Markdown export.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, type MockOptions } from "../src/mock";
import { applyEvent, depthLine, estimateMinutes, newRun } from "../src/state";
import { runMarkdown } from "../src/session";

let root: HTMLElement;

beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
});

const tick = async (n = 12) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};

async function start(opts: MockOptions = {}) {
  const api = mockApi({ delay: 0, keys: ["claude", "gemini"], onboarded: true, ...opts });
  const app = new App(api, root);
  await app.start({ view: "main" });
  await tick();
  return { app, api };
}

const text = () => root.textContent ?? "";
const buttonNamed = (name: string) => [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;
const click = async (name: string) => {
  const b = buttonNamed(name);
  if (!b) throw new Error(`no button "${name}" in: ${text()}`);
  b.click();
  await tick();
};
const type = async (sel: string, value: string) => {
  const el = root.querySelector<HTMLTextAreaElement>(sel)!;
  el.value = value;
  el.dispatchEvent(new Event("input"));
  await tick(2);
};

describe("the mode control", () => {
  it("is Quick by default and says how long", async () => {
    const { app } = await start();
    expect(app.state.ask.depth).toBe("quick");
    expect(root.querySelector('[data-depth="quick"]')?.getAttribute("aria-checked")).toBe("true");
    expect(root.querySelector("[data-depth-hint]")?.textContent).toContain("Quick: about 2 minutes.");
  });

  it("Deep Think shows its time before asking, always runs the coach, and sends --mode deep with four rounds", async () => {
    const { app, api } = await start();
    app.state.ask = { ...app.state.ask, coach: false };
    app.render();
    root.querySelector<HTMLButtonElement>('[data-depth="deep"]')!.click();
    await tick();
    expect(text()).toContain("Deep Think: about 14 minutes.");
    // The coach switch is on and locked.
    const sw = [...root.querySelectorAll<HTMLInputElement>("input.switch")].find((i) => i.closest(".locked"));
    expect(sw?.checked).toBe(true);
    expect(sw?.disabled).toBe(true);
    await type("textarea.question", "Bread or garden?");
    await click("Sharpen and think deeply");
    expect(app.state.coach).not.toBeNull();
    await click("Write the brief");
    expect(root.querySelector("[data-deep-on]")?.textContent).toContain("Deep Think: about 14 minutes.");
    await click("Ask the table");
    await tick(60);
    expect(api.calls.find((c) => c.startsWith("askStart:"))).toMatch(/^askStart:claude,gemini:4/);
    expect(api.calls).toContain("mode:deep");
    expect(app.state.run?.depth).toBe("deep");
    expect(root.querySelector(".final-verdict [data-depth]")?.textContent).toBe("Deep Think");
  });
});

describe("the coach suggests Deep Think", () => {
  it("on a complex brief; Use Deep Think switches the run", async () => {
    const { app, api } = await start();
    await type("textarea.question", "Should we sign the three-year vendor contract?");
    await click("Sharpen and ask");
    await click("Write the brief");
    expect(text()).toContain("The coach suggests Deep Think");
    expect(text()).toContain("Money and legal stakes, and several people affected.");
    await click("Use Deep Think");
    expect(app.state.ask.depth).toBe("deep");
    expect(text()).not.toContain("The coach suggests Deep Think");
    await click("Ask the table");
    await tick(60);
    expect(api.calls).toContain("mode:deep");
    expect(api.calls.find((c) => c.startsWith("askStart:"))).toMatch(/^askStart:claude,gemini:4/);
  });

  it("Keep Quick keeps the run quick", async () => {
    const { app, api } = await start();
    await type("textarea.question", "Should we sign the three-year vendor contract?");
    await click("Sharpen and ask");
    await click("Write the brief");
    await click("Keep Quick");
    expect(text()).not.toContain("The coach suggests Deep Think");
    await click("Ask the table");
    await tick(30);
    expect(app.state.ask.depth).toBe("quick");
    expect(api.calls).toContain("mode:quick");
    expect(api.calls.find((c) => c.startsWith("askStart:"))).toMatch(/^askStart:claude,gemini:1/);
  });

  it("not on a simple one", async () => {
    await start();
    await type("textarea.question", "Bread or garden?");
    await click("Sharpen and ask");
    await click("Write the brief");
    expect(text()).not.toContain("The coach suggests Deep Think");
  });
});

describe("estimates and export", () => {
  it("uses the engine's numbers", () => {
    expect(estimateMinutes("quick")).toBe(2);
    expect(estimateMinutes("deep")).toBe(14);
    expect(estimateMinutes("deep", 2)).toBe(8);
    expect(depthLine("deep")).toBe("Deep Think: about 14 minutes.");
  });

  it("puts the mode in the Markdown export", () => {
    let r = newRun("Q", ["claude", "gemini"], 4);
    r = applyEvent(r, { event: "run_started", heads: ["claude", "gemini"], rounds: 4, mode: "deliberation", depth: "deep" });
    expect(runMarkdown(r, { date: null })).toContain("**Mode:** Deep Think, up to 4 rounds");
    expect(runMarkdown(newRun("Q", ["claude"], 1), { date: null })).toContain("**Mode:** Quick, one round");
  });
});
