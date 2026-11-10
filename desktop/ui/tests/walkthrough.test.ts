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
    await click("More AIs");
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
    expect(text()).toContain("A chat plan like Claude Pro or ChatGPT Plus is not an API key. Use the link on a card to get a key.");
  });
});

describe("screen 3, AI apps found on this computer", () => {
  const cards = () => [...root.querySelectorAll<HTMLElement>(".cli-card")];
  const card = (id: string) => root.querySelector<HTMLElement>(`[data-cli="${id}"]`)!;
  const box = (id: string) => card(id).querySelector<HTMLInputElement>("input.cli-check")!;

  it("lists each tool found, ticks the signed-in ones, and allows Continue with no key", async () => {
    const { app, api } = await start({ clis: "some" }, { screen: 3 });
    expect(api.calls).toContain("clisDetect");
    expect(root.querySelector("h1")?.textContent).toBe("We found these on your computer");
    expect(cards().map((c) => c.getAttribute("data-cli"))).toEqual(["claude", "codex", "gemini", "grok", "cursor"]);
    expect(card("claude").textContent).toContain("Claude Code, signed in. Uses your Claude plan.");
    expect(box("claude").checked).toBe(true);
    expect(box("codex").checked).toBe(true);
    expect(box("grok").checked).toBe(false);
    expect(box("grok").disabled).toBe(true);
    expect(app.state.settings.cli_heads).toEqual(["claude", "codex", "gemini"]);
    expect(buttonNamed("Continue")!.disabled).toBe(false);
  });

  it("keeps every card to two sentences at most, in plain words", async () => {
    await start({ clis: "some" }, { screen: 3 });
    for (const c of cards()) {
      const line = c.querySelector(".cli-line")!.textContent!;
      expect(line.split(/(?<=\.)\s/).length).toBeLessThanOrEqual(2);
      expect(line).not.toMatch(/OAuth|CLI flag|token|\u2014/);
    }
  });

  it("folds the API key cards under Or use an API key, with the reworded plan note", async () => {
    await start({ clis: "some" }, { screen: 3 });
    expect(root.querySelector(".ai-card")).toBeNull();
    expect(text()).not.toContain("is not an API key");
    await click("Or use an API key");
    expect(root.querySelectorAll(".ai-card").length).toBe(2);
    expect(text()).toContain("A chat plan like Claude Pro or ChatGPT Plus is not an API key. Your plan already works through the apps above.");
  });

  it("Continue turns off when every tool is unticked and no key is set", async () => {
    const { app } = await start({ clis: "some" }, { screen: 3 });
    for (const id of ["claude", "codex", "gemini"]) {
      box(id).checked = false;
      box(id).dispatchEvent(new Event("change"));
      await tick();
    }
    expect(app.state.settings.cli_heads).toEqual([]);
    expect(app.state.settings.clis_chosen).toBe(true);
    expect(buttonNamed("Continue")!.disabled).toBe(true);
    box("gemini").checked = true;
    box("gemini").dispatchEvent(new Event("change"));
    await tick();
    expect(buttonNamed("Continue")!.disabled).toBe(false);
  });

  it("Sign in opens the tool's own sign-in, then Check again ticks it", async () => {
    const { app, api } = await start({ clis: "some" }, { screen: 3 });
    expect(card("grok").textContent).toContain("Grok is installed but not signed in. Click Sign in to open it in Terminal.");
    card("grok").querySelector<HTMLButtonElement>("button")!.click();
    await tick();
    expect(api.calls).toContain("cliLogin:grok");
    expect(card("grok").textContent).toContain("Finish signing in in the new window, then click Check again.");
    await click("Check again");
    await tick();
    expect(card("grok").textContent).toContain("Grok, signed in. Uses your Grok plan.");
    expect(box("grok").checked).toBe(true);
    expect(app.state.settings.cli_heads).toContain("grok");
  });

  it("asks the table through the ticked tools", async () => {
    const { api } = await start({ clis: "some" }, { screen: 5 });
    await click("Ask the table");
    await tick(20);
    expect(api.calls).toContain("askStart:claude,codex,gemini:1");
    expect(root.querySelectorAll(".answer.answered")).toHaveLength(3);
  });

  it("with no tools, says so and opens the key cards", async () => {
    await start({ clis: "none" }, { screen: 3 });
    expect(root.querySelector("h1")?.textContent).toBe("Connect at least one AI");
    expect(text()).toContain("We did not find Claude Code, Codex or Gemini CLI on this computer.");
    expect(root.querySelectorAll(".ai-card").length).toBe(2);
  });

  it("says a terminal window on Windows", async () => {
    await start({ clis: "some", os: "windows" }, { screen: 3 });
    expect(card("cursor").textContent).toContain("Click Sign in to open it in a terminal window.");
  });
});

describe("settings, AI accounts", () => {
  it("shows the same list of apps, refreshable, and seats ticked tools at the table", async () => {
    const { api } = await start({ clis: "some", keys: ["openai"], onboarded: true }, { view: "settings" });
    expect(text()).toContain("Apps on this computer");
    expect(root.querySelectorAll(".cli-card").length).toBe(5);
    const before = api.calls.filter((c) => c === "clisDetect").length;
    await click("Look again");
    expect(api.calls.filter((c) => c === "clisDetect").length).toBe(before + 1);
    const seats = [...root.querySelectorAll(".switch-row")].map((r) => r.textContent);
    expect(seats).toEqual(expect.arrayContaining(["Claude", "Codex", "Gemini", "ChatGPT"]));
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
