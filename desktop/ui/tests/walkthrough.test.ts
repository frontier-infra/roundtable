// Drives the real screens in jsdom against the mock backend, one walkthrough state at a time.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, type MockOptions } from "../src/mock";
import { renderMarkdown } from "../src/markdown";

let root: HTMLElement;

beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
});

const tick = async (n = 6) => {
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
const buttonNamed = (name: string) => [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;
const click = async (name: string) => {
  const b = buttonNamed(name);
  if (!b) throw new Error(`no button "${name}" in: ${text()}`);
  b.click();
  await tick();
};

async function paste(head: string, value: string) {
  const input = root.querySelector<HTMLInputElement>(`[data-head="${head}"] input.key-input`)!;
  input.value = value;
  input.dispatchEvent(new Event("input"));
  input.dispatchEvent(new Event("paste"));
  await tick(10);
}

describe("screen 1, welcome", () => {
  it("shows the promise and the cost note, and Get started moves on", async () => {
    await start();
    expect(root.querySelector("h1")?.textContent).toBe("Ask several AIs at once. Get one clear answer.");
    expect(text()).toContain("Roundtable itself is free.");
    expect(buttonNamed("Back")).toBeUndefined();
    await click("Get started");
    expect(root.querySelector("h1")?.textContent).toBe("Set up Roundtable");
    expect(text()).toContain("Step 2 of 5");
  });
});

describe("screen 2, set up", () => {
  it("ticks three lines green, then allows Continue", async () => {
    const { api } = await start({}, { screen: 2 });
    expect(buttonNamed("Continue")!.disabled).toBe(true);
    expect(buttonNamed("Skip for now")).toBeUndefined();
    await click("Set up");
    await tick(10);
    expect(api.calls).toContain("engineSetup");
    expect(root.querySelectorAll(".ticks li.done")).toHaveLength(3);
    expect(text()).toContain("All set.");
    expect(buttonNamed("Continue")!.disabled).toBe(false);
  });

  it("on failure offers Try again and Get help with the error code", async () => {
    const { api } = await start({ setupFails: true }, { screen: 2 });
    await click("Set up");
    await tick(10);
    expect(text()).toContain("Setup did not finish. Click Try again.");
    expect(text()).toContain("E_SELFTEST");
    await click("Try again");
    await tick(10);
    expect(text()).toContain("Click Get help");
    await click("Get help");
    expect(api.calls.some((c) => c.startsWith("open:") && c.includes("E_SELFTEST"))).toBe(true);
  });
});

describe("screen 3, connect AI accounts", () => {
  it("shows Claude and Gemini first, the rest under More AIs", async () => {
    await start({}, { screen: 3 });
    const heads = () => [...root.querySelectorAll(".ai-card")].map((c) => c.getAttribute("data-head"));
    expect(heads()).toEqual(["claude", "gemini"]);
    expect(text()).toContain("Recommended. Claude also acts as the chair.");
    await click("▸ More AIs");
    expect(heads()).toEqual(["claude", "gemini", "openai", "grok", "glm", "minimax"]);
  });

  it("checks a pasted key at once and says where it was saved", async () => {
    const { api } = await start({}, { screen: 3 });
    expect(buttonNamed("Continue")!.disabled).toBe(true);
    await paste("claude", "sk-ant-not-a-real-key");
    expect(api.calls).toContain("keySave:claude");
    expect(text()).toContain("Connected. Saved in your Mac's Keychain.");
    expect(buttonNamed("Continue")!.disabled).toBe(false);
  });

  it("names Windows Credential Manager on Windows", async () => {
    await start({ os: "windows" }, { screen: 3 });
    await paste("gemini", "AIza-not-a-real-key");
    expect(text()).toContain("Saved in Windows Credential Manager.");
  });

  it("shows a plain line for a bad key and for no credit", async () => {
    await start({}, { screen: 3 });
    await paste("claude", "sk-bad-key-123");
    expect(text()).toContain("That key did not work. Check you copied all of it.");
    await paste("gemini", "AIza-broke-account");
    expect(text()).toContain("That account has no credit left.");
    expect(buttonNamed("Continue")!.disabled).toBe(true);
  });

  it("offers a key page link per AI and the chat-plan note", async () => {
    const { api } = await start({}, { screen: 3 });
    const link = root.querySelector<HTMLAnchorElement>('[data-head="claude"] .how a')!;
    link.click();
    expect(api.calls).toContain("open:https://console.anthropic.com/settings/keys");
    expect(text()).toContain("A Claude or ChatGPT chat plan is not an API key.");
  });
});

describe("screen 4, connect to Claude", () => {
  it("connects the Claude app and Claude Code, one button each", async () => {
    const { api } = await start({ claude: "both" }, { screen: 4 });
    expect(text()).toContain("Claude app found.");
    expect(text()).toContain("Claude Code found.");
    await click("Connect to the Claude app");
    expect(api.calls).toContain("claudeConnectApp");
    expect(text()).toContain("Done. Quit and reopen Claude to finish.");
    await click("Connect to Claude Code");
    expect(text()).toContain("Done. Start a new Claude Code session to use it.");
    expect(root.querySelectorAll(".connect-row.done")).toHaveLength(2);
  });

  it("does nothing until a button is pressed", async () => {
    const { api } = await start({ claude: "both" }, { screen: 4 });
    expect(api.calls.filter((c) => c.startsWith("claudeConnect"))).toHaveLength(0);
  });

  it("without Claude, shows the download link and the two plugin lines", async () => {
    const { api } = await start({ claude: "none" }, { screen: 4 });
    expect(text()).toContain("Claude is not on this computer yet.");
    expect(root.querySelector(".copy-box pre")?.textContent).toBe(
      "/plugin marketplace add frontier-infra/roundtable\n/plugin install roundtable@frontier-infra",
    );
    await click("Copy");
    expect(api.calls.some((c) => c.startsWith("copy:"))).toBe(true);
  });

  it("offers to turn off the old plugin so Claude sees one tool", async () => {
    const { api } = await start({ claude: "plugin" }, { screen: 4 });
    expect(text()).toContain("Claude would see two Roundtable tools.");
    await click("Use the app's engine instead");
    expect(api.calls).toContain("claudeDisablePlugin");
    expect(text()).not.toContain("Claude would see two Roundtable tools.");
  });

  it("Skip for now moves on", async () => {
    await start({ claude: "none" }, { screen: 4 });
    await click("Skip for now");
    expect(root.querySelector("h1")?.textContent).toBe("Try it now");
  });
});

describe("screen 5, first roundtable", () => {
  it("fills in a sample question, shows live cards, then finishes setup", async () => {
    const { app, api } = await start({ keys: ["claude", "gemini"] }, { screen: 5 });
    const q = root.querySelector<HTMLTextAreaElement>("textarea.question")!;
    expect(q.value).toContain("bake bread or start a vegetable garden");
    await click("Ask the table");
    await tick(20);
    expect(api.calls).toContain("askStart:claude,gemini:1");
    expect(root.querySelectorAll(".answer.answered")).toHaveLength(2);
    expect(text()).toContain("That's it. You're set up.");
    await click("Open Roundtable");
    expect(app.state.view).toBe("main");
    expect(app.state.settings.onboarded).toBe(true);
    expect(root.querySelector(".history li")).not.toBeNull();
  });

  it("when every AI fails, shows the first error and Back to keys", async () => {
    const { app } = await start({ keys: [] }, { screen: 5 });
    app.state.keys.claude = { status: "connected" };
    app.render();
    await click("Ask the table");
    await tick(20);
    expect(text()).toContain("That key did not work.");
    await click("Back to keys");
    expect(app.state.step).toBe(3);
  });
});

describe("main screen", () => {
  it("updates the request count with the mode and runs Talk it through to a verdict", async () => {
    const { app } = await start({ keys: ["claude", "gemini", "openai"], onboarded: true }, { view: "main" });
    expect(text()).toContain("This will ask 3 AIs, about 3 requests.");
    root.querySelector<HTMLButtonElement>('[data-mode="talk"]')!.click();
    await tick();
    expect(text()).toContain("at most 12 requests");
    const q = root.querySelector<HTMLTextAreaElement>("textarea.question")!;
    q.value = "Garden or bread?";
    q.dispatchEvent(new Event("input"));
    await click("Ask the table");
    await tick(30);
    expect(app.state.run?.done).toBe(true);
    expect(root.querySelector(".verdict h2")?.textContent).toBe("They agree");
    // The engine asks for "POSITION:" in later rounds; people do not need to see it.
    expect(root.querySelector(".answer .md")?.textContent).not.toMatch(/^POSITION:/);
    expect(buttonNamed("Copy all")).toBeDefined();
    expect(buttonNamed("Ask a follow-up")).toBeDefined();
  });

  it("lists past roundtables with a Claude badge", async () => {
    await start(
      { keys: ["claude"], onboarded: true, history: [{ id: "100-abcd", ts: 100, question: "From Claude", source: "claude", mode: "advisory", heads: ["claude"] }] },
      { view: "main" },
    );
    expect(root.querySelector(".history .badge")?.textContent).toBe("Claude");
  });
});

describe("markdown", () => {
  it("never turns answer text into HTML", () => {
    const el = renderMarkdown('**bold** <img src=x onerror="alert(1)"> `code`');
    expect(el.querySelector("img")).toBeNull();
    expect(el.querySelector("strong")?.textContent).toBe("bold");
    expect(el.textContent).toContain("<img");
  });
});
