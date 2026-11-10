// Use Roundtable from your AI: Codex and Cursor in one click, snippets for other apps,
// and the Share with ChatGPT switch (off by default, a link to copy, every call logged).

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, type MockOptions } from "../src/mock";
import { logLine } from "../src/views/connect";

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
});
const tick = async (n = 10) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};
async function settings(opts: MockOptions = {}) {
  const api = mockApi({ delay: 0, onboarded: true, ...opts });
  const app = new App(api, root);
  await app.start({ view: "main" });
  await app.openSettings();
  await tick();
  return { app, api };
}
const text = () => root.textContent ?? "";
const buttonNamed = (name: string) => [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;
const shareSwitch = () =>
  [...root.querySelectorAll("label.switch-row")].find((l) => l.querySelector(".switch-text > span")?.textContent === "Share with ChatGPT")?.querySelector("input") as HTMLInputElement | undefined;

describe("other AI apps", () => {
  it("connects Codex and Cursor with one click each, and disconnects", async () => {
    const { app, api } = await settings();
    expect(text()).toContain("Use Roundtable from your AI");
    expect(text()).toContain("Codex found.");
    expect(text()).toContain("Cursor found.");
    buttonNamed("Connect to Codex")!.click();
    await tick();
    expect(api.calls).toContain("clientConnect:codex");
    expect(text()).toContain("Start a new Codex session");
    buttonNamed("Connect to Cursor")!.click();
    await tick();
    expect(api.calls).toContain("clientConnect:cursor");
    const row = root.querySelector('[data-target="codex"]')!;
    (row.querySelector("button") as HTMLButtonElement).click();
    await tick();
    expect(api.calls).toContain("clientDisconnect:codex");
    expect(app.state.clients.codex_connected).toBe(false);
  });

  it("hides rows for apps that are not here and still offers the snippets", async () => {
    await settings({ clients: "none" });
    expect(text()).not.toContain("Codex found.");
    expect(text()).toContain("Another AI app? Copy its setup");
    expect(root.querySelector('[data-snippet="codex"] pre')?.textContent).toContain("tool_timeout_sec");
  });
});

describe("Share with ChatGPT", () => {
  it("is off by default and starts nothing until switched on", async () => {
    const { api } = await settings();
    expect(shareSwitch()!.checked).toBe(false);
    expect(api.calls).not.toContain("shareStart");
  });

  it("switching on shows the link to paste; switching off stops it", async () => {
    const { app, api } = await settings();
    const sw = shareSwitch()!;
    sw.checked = true;
    sw.dispatchEvent(new Event("change"));
    await tick();
    expect(api.calls).toContain("shareStart");
    expect(text()).toContain("Developer mode");
    expect(root.textContent).toContain("https://quiet-river-test.trycloudflare.com/mcp/");
    expect(buttonNamed("Copy link")).toBeTruthy();
    api.shareLog({ type: "call_started", ts: 3, question: "Should we ship on Friday?" });
    api.shareLog({ type: "request", ts: 4, method: "tools/call", status: 200, question: "Should we ship on Friday?", result: "Roundtable: 3 of 3 seats answered, 1 round.", is_error: false, ms: 61000 });
    api.shareLog({ type: "request", ts: 5, status: 401, method: "", peer: "203.0.113.9" });
    await app.refreshShare();
    expect(text()).toContain("Roundtable finished: \"Should we ship on Friday?\"");
    expect(text()).toContain("Refused: wrong or missing token (from 203.0.113.9).");
    const off = shareSwitch()!;
    off.checked = false;
    off.dispatchEvent(new Event("change"));
    await tick();
    expect(api.calls).toContain("shareStop");
    expect(app.state.share.status?.running).toBe(false);
    expect(root.textContent).not.toContain("https://quiet-river-test.trycloudflare.com/mcp/f");
  });

  it("says what to install when cloudflared is missing", async () => {
    await settings({ noCloudflared: true });
    expect(text()).toContain("brew install cloudflared");
    const sw = shareSwitch()!;
    sw.checked = true;
    sw.dispatchEvent(new Event("change"));
    await tick();
    expect(root.querySelector('[role="alert"]')?.textContent).toContain("cloudflared");
  });

  it("writes plain log lines with no em dashes", () => {
    expect(logLine({ type: "request", status: 429, question: "Q" })).toContain("Refused: rate limit for \"Q\"");
    expect(logLine({ type: "call_started", question: "x".repeat(100) })).toContain("...");
    expect(logLine({ type: "stopped", note: "Sharing stopped." })).toContain("Sharing stopped.");
    expect(logLine({ type: "request", method: "tools/call", status: 200, is_error: true, question: "Q" })).toContain("failed");
    for (const l of [logLine({ type: "request", status: 403, note: "origin x" })]) expect(l).not.toMatch(/—/);
  });
});
