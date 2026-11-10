// Heads that run by the route that works, say why when they do not, and can be retried.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, sampleClis, type MockOptions } from "../src/mock";
import {
  applyEvent,
  canRetry,
  cardStatusText,
  cliHeadsOn,
  defaultSettings,
  firstSection,
  newRun,
  replaceSection,
  testLine,
  TEST_TRIES,
} from "../src/state";

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
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
const buttonNamed = (name: string, scope: ParentNode = root) =>
  [...scope.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;

const SIGN_IN = { message: "Kimi says you are not signed in. Sign in to Kimi again.", fix: "sign_in" as const };
const GOOGLE = { message: "Google no longer lets Gemini CLI answer on a personal Google sign-in. Add a Gemini key instead.", fix: "add_key" as const };

describe("state helpers", () => {
  it("shows live state words, not a key complaint", () => {
    const c = { head: "claude", status: "thinking" as const, text: "", round: 1 };
    expect(cardStatusText(c)).toBe("Asking...");
    expect(cardStatusText({ ...c, status: "skipped" })).toBe("Not set up");
    expect(cardStatusText({ ...c, status: "failed" })).toBe("Failed");
  });

  it("keeps the reason and the fix from the engine", () => {
    let r = newRun("Q", ["kimi"], "quick");
    r = applyEvent(r, { event: "head_done", head: "kimi", round: 1, status: "failed", text: "", message: SIGN_IN.message, fix: "sign_in", via: "Kimi" });
    expect(r.cards.kimi.message).toBe(SIGN_IN.message);
    expect(r.cards.kimi.fix).toBe("sign_in");
    expect(canRetry(r, r.cards.kimi)).toBe(false);
    r = applyEvent(r, { event: "process_exit" });
    expect(canRetry(r, r.cards.kimi)).toBe(true);
    expect(canRetry({ ...r, mode: "talk" }, r.cards.kimi)).toBe(false);
  });

  it("swaps one head's section after a retry and leaves the rest", () => {
    const t = "# T\n\n---\n## A\n\nfirst\n\n---\n## B\n\n_(no answer)_\n\n---\n_footer_\n";
    const retry = "# T\n\n---\n## B (your plan)\n\nnow it answers\n\n---\n_footer_\n";
    expect(firstSection(retry)).toBe("## B (your plan)\n\nnow it answers\n\n");
    const out = replaceSection(t, 1, firstSection(retry));
    expect(out).toBe("# T\n\n---\n## A\n\nfirst\n\n---\n## B (your plan)\n\nnow it answers\n\n---\n_footer_\n");
    expect(replaceSection(t, 5, "x")).toBe(t);
  });

  it("leaves a tool out of the table after two failed checks", () => {
    const s = { clis: { loaded: true, loading: false, list: sampleClis(), signingIn: [] }, settings: { ...defaultSettings(), cli_heads: ["claude", "gemini"] }, tests: {} };
    expect(cliHeadsOn(s)).toEqual(["claude", "gemini"]);
    const tests = { "app:gemini": { status: "failed" as const, failures: TEST_TRIES } };
    expect(cliHeadsOn({ ...s, tests })).toEqual(["claude"]);
  });

  it("writes the test line in plain words", () => {
    expect(testLine(undefined)).toBeNull();
    expect(testLine({ status: "testing", failures: 0 })).toBe("Checking with a five-word question...");
    const ok = { head: "claude", name: "Claude", ok: true, status: "answered", via: "Claude Code", first_line: "Yes.", message: null, fix: null, secs: 4.6, command: null, exit: 0 };
    expect(testLine({ status: "ok", failures: 0, result: ok })).toBe("Works. Answered in 4.6 s through Claude Code: “Yes.”");
    expect(testLine({ status: "failed", failures: 2, unticked: true, result: { ...ok, ok: false, message: GOOGLE.message } })).toBe(`Unticked: ${GOOGLE.message}`);
  });
});

describe("setup checks each ticked tool with a five-word question", () => {
  it("tests every ticked tool and unticks one that fails twice, with the reason", async () => {
    const { app, api } = await start({ clis: "some", testFails: { gemini: GOOGLE } }, { screen: 3 });
    await tick(20);
    expect(api.calls.filter((c) => c === "headTest:claude:app")).toHaveLength(1);
    expect(api.calls.filter((c) => c === "headTest:gemini:app")).toHaveLength(2);
    expect(app.state.settings.cli_heads).toEqual(["claude", "codex"]);
    const g = root.querySelector('[data-cli="gemini"]')!;
    expect(g.querySelector<HTMLInputElement>("input.cli-check")!.checked).toBe(false);
    expect(g.textContent).toContain(`Unticked: ${GOOGLE.message}`);
    expect(buttonNamed("Add a key", g)).toBeDefined();
    expect(root.querySelector('[data-cli="claude"]')!.textContent).toContain("Works. Answered in 1.2 s");
  });

  it("the first question lists only heads that will answer", async () => {
    const { app } = await start({ clis: "some", testFails: { gemini: GOOGLE } }, { screen: 3 });
    await tick(20);
    app.set({ step: 5 });
    expect(text()).toContain("These will answer: Claude, Codex.");
    expect(text()).toContain("Left out because they did not answer the check: Gemini.");
  });
});

describe("a failed head says what to do and can be retried", () => {
  it("shows the reason, a Sign in button and Retry; Retry asks only that head", async () => {
    const tools = [...sampleClis(), { id: "kimi", head: "kimi", name: "Kimi", found: true, version: "2.5.2", signed_in: true, note: "Uses your Kimi account.", how: "kimi -p", login: "kimi login" }].filter(
      (d) => d.id !== "kimi" || d.found,
    );
    const { app, api } = await start({ clis: tools, onboarded: true, runFails: { kimi: SIGN_IN } }, { view: "main" });
    await app.saveSettings({ cli_heads: ["claude", "kimi"] });
    await app.ask("Bread or garden?", "quick", false);
    await tick(30);
    expect(api.calls).toContain("askStart:claude,kimi:1");
    const k = root.querySelector('.answer[data-head="kimi"]')!;
    expect(k.textContent).toContain("Failed");
    expect(k.textContent).toContain(SIGN_IN.message);
    expect(buttonNamed("Sign in to Kimi", k)).toBeDefined();
    expect(root.querySelector('.answer[data-head="claude"]')!.textContent).toContain("Answered through Claude Code.");
    buttonNamed("Retry", k)!.click();
    await tick(20);
    expect(api.calls).toContain("askRetry:kimi");
    expect(api.calls.filter((c) => c.startsWith("askStart"))).toHaveLength(1);
    expect(app.state.run!.cards.kimi.status).toBe("answered");
    expect(app.state.run!.cards.claude.status).toBe("answered");
  });

  it("Sign in on a card opens the tool's own sign-in", async () => {
    const tools = [{ id: "kimi", head: "kimi", name: "Kimi", found: true, version: "2.5.2", signed_in: true, note: "n", how: "kimi -p", login: "kimi login" }];
    const { app, api } = await start({ clis: tools, onboarded: true, runFails: { kimi: SIGN_IN } }, { view: "main" });
    await app.saveSettings({ cli_heads: ["kimi"] });
    await app.ask("Q", "quick", false);
    await tick(30);
    buttonNamed("Sign in to Kimi")!.click();
    await tick();
    expect(api.calls).toContain("cliLogin:kimi");
  });
});

describe("settings, AI accounts, Test button", () => {
  it("runs the five-word prompt per AI and shows the result", async () => {
    const { api } = await start({ clis: "some", keys: ["openai"], onboarded: true, testFails: { codex: { message: "Codex hit its usage limit. Try again later.", fix: "retry" } } }, { view: "settings" });
    await tick(20);
    const claude = root.querySelector('[data-cli="claude"]')!;
    buttonNamed("Test", claude)!.click();
    await tick(20);
    expect(api.calls).toContain("headTest:claude:app");
    expect(root.querySelector('[data-cli="claude"]')!.textContent).toContain("Works. Answered in 1.2 s through Claude Code: “Yes, water is wet.”");
    const codex = root.querySelector('[data-cli="codex"]')!;
    buttonNamed("Test", codex)!.click();
    await tick(20);
    expect(root.querySelector('[data-cli="codex"]')!.textContent).toContain("Codex hit its usage limit. Try again later.");
    // A manual test never unticks.
    expect(api.calls.filter((c) => c === "headTest:codex:app")).toHaveLength(1);
    const openai = root.querySelector('.ai-card[data-head="openai"]')!;
    buttonNamed("Test", openai)!.click();
    await tick(20);
    expect(api.calls).toContain("headTest:openai:key");
    expect(root.querySelector('.ai-card[data-head="openai"]')!.textContent).toContain("through API key");
  });
});
