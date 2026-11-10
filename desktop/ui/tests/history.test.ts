// Deleting past roundtables: one, several, Undo, Clear all, the keyboard, and what never
// becomes a roundtable. Drives the real screens in jsdom against the mock backend.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/app";
import { mockApi } from "../src/mock";
import type { HistoryEntry } from "../src/api";
import { deleteTargets, flagOnly, FIRST_QUESTION, pickRows, runSource, UNDO_MS } from "../src/state";
import { clearAllQuestion } from "../src/views/settings";

let root: HTMLElement;

beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const tick = async (n = 8) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};

function rows(n = 5): HistoryEntry[] {
  return Array.from({ length: n }, (_, i) => ({ id: `${1000 - i}-a${i}`, ts: 1000 - i, question: `Question ${i + 1}?`, source: "app", mode: "advisory", heads: ["claude"] }));
}

async function start(view: "main" | "settings" = "main", history = rows()) {
  const api = mockApi({ delay: 0, keys: ["claude"], onboarded: true, history });
  const app = new App(api, root);
  await app.start({ view });
  await tick();
  return { app, api };
}

const shown = () => [...root.querySelectorAll<HTMLElement>(".history li")].map((li) => li.querySelector(".q")!.textContent);
const row = (q: string) => [...root.querySelectorAll<HTMLElement>(".history li")].find((li) => li.querySelector(".q")!.textContent === q)!;
const toast = () => root.querySelector(".toast");
const undo = () => root.querySelector<HTMLButtonElement>(".toast-action");
const byLabel = (label: string) => root.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
const press = (el: HTMLElement, key: string, more: KeyboardEventInit = {}) => el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, ...more }));
const clickWith = (el: HTMLElement, more: MouseEventInit = {}) => el.dispatchEvent(new MouseEvent("click", { bubbles: true, ...more }));

describe("delete one roundtable", () => {
  it("from the row's delete button, with Undo that brings it back whole", async () => {
    const { app, api } = await start();
    const before = await api.historyGet("999-a1");
    byLabel("Delete Question 2?")!.click();
    await tick();
    expect(shown()).toEqual(["Question 1?", "Question 3?", "Question 4?", "Question 5?"]);
    expect(api.calls).toContain("historyDelete:999-a1");
    expect(await api.historyGet("999-a1")).toBeNull();
    expect(toast()?.textContent).toContain("Deleted.");
    expect(undo()?.getAttribute("aria-label")).toBe("Undo delete of 1 roundtable");
    undo()!.click();
    await tick();
    expect(shown()).toEqual(["Question 1?", "Question 2?", "Question 3?", "Question 4?", "Question 5?"]);
    expect(await api.historyGet("999-a1")).toBe(before);
    expect(toast()).toBeNull();
    expect(app.state.undo).toBeNull();
  });

  it("from the right-click menu", async () => {
    const { api } = await start();
    row("Question 3?").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 40, clientY: 80 }));
    const menu = document.querySelector<HTMLElement>(".menu[role=menu]")!;
    expect([...menu.querySelectorAll("button")].map((b) => b.textContent)).toEqual(["Open", "Delete"]);
    expect(document.activeElement).toBe(menu.querySelector("button"));
    menu.querySelector<HTMLButtonElement>(".menu-item.danger")!.click();
    await tick();
    expect(shown()).not.toContain("Question 3?");
    expect(api.calls).toContain("historyDelete:998-a2");
  });

  it("from the open roundtable's header, and Undo opens it again", async () => {
    const { app } = await start();
    row("Question 1?").click();
    await tick();
    expect(app.state.opened?.id).toBe("1000-a0");
    const del = byLabel("Delete this roundtable")!;
    expect(del.textContent).toBe("Delete");
    del.click();
    await tick();
    expect(app.state.opened).toBeNull();
    expect(root.querySelector("textarea.question.big")).not.toBeNull();
    undo()!.click();
    await tick();
    expect(app.state.opened?.id).toBe("1000-a0");
    expect(row("Question 1?").classList.contains("active")).toBe(true);
  });

  it("deletes the files for real once the Undo window closes", async () => {
    vi.useFakeTimers();
    const api = mockApi({ delay: 0, keys: ["claude"], onboarded: true, history: rows(2) });
    const app = new App(api, root);
    await app.start({ view: "main" });
    await vi.advanceTimersByTimeAsync(0);
    byLabel("Delete Question 1?")!.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(undo()).not.toBeNull();
    await vi.advanceTimersByTimeAsync(UNDO_MS - 500);
    expect(undo()).not.toBeNull();
    expect(api.calls.some((c) => c.startsWith("historyPurge:"))).toBe(false);
    await vi.advanceTimersByTimeAsync(600);
    expect(api.calls.some((c) => c.startsWith("historyPurge:"))).toBe(true);
    expect(toast()).toBeNull();
    // Too late: nothing to undo, the roundtable stays gone.
    await expect(api.historyRestore(api.calls.find((c) => c.startsWith("historyPurge:"))!.split(":")[1])).rejects.toThrow();
    expect(app.state.history.map((e) => e.id)).toEqual(["999-a1"]);
  });

  it("Command Z undoes the last delete", async () => {
    await start();
    byLabel("Delete Question 1?")!.click();
    await tick();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", metaKey: true }));
    await tick();
    expect(shown()[0]).toBe("Question 1?");
  });
});

describe("delete several", () => {
  it("command click and shift click pick rows, the menu deletes them together, Undo restores all", async () => {
    const { app, api } = await start();
    row("Question 1?").click();
    await tick();
    clickWith(row("Question 2?"), { metaKey: true });
    clickWith(row("Question 4?"), { shiftKey: true });
    await tick();
    expect(app.state.historySel.sort()).toEqual(["1000-a0", "997-a3", "998-a2", "999-a1"]);
    expect(root.querySelector(".select-count")?.textContent).toBe("4 selected");
    row("Question 3?").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 10, clientY: 10 }));
    const del = document.querySelector<HTMLButtonElement>(".menu .menu-item.danger")!;
    expect(del.textContent).toBe("Delete 4 roundtables");
    del.click();
    await tick();
    expect(shown()).toEqual(["Question 5?"]);
    expect(toast()?.textContent).toContain("Deleted 4 roundtables.");
    expect(api.calls.filter((c) => c.startsWith("historyDelete:"))).toHaveLength(1);
    undo()!.click();
    await tick();
    expect(shown()).toHaveLength(5);
    expect(app.state.selecting).toBe(false);
  });

  it("Select mode: checkboxes, All, Delete, Done", async () => {
    const { app } = await start();
    byLabel("Select roundtables to delete")!.click();
    await tick();
    expect(root.querySelectorAll(".history .row-check")).toHaveLength(5);
    expect(root.querySelector(".history .row-delete")).toBeNull();
    row("Question 2?").click();
    row("Question 5?").click();
    await tick();
    expect(app.state.opened).toBeNull();
    expect(row("Question 2?").getAttribute("aria-selected")).toBe("true");
    expect(byLabel("Delete 2 roundtables")).not.toBeNull();
    byLabel("Select all")!.click();
    await tick();
    byLabel("Delete 5 roundtables")!.click();
    await tick();
    expect(shown()).toEqual([]);
    expect(app.state.selecting).toBe(false);
    undo()!.click();
    await tick();
    expect(shown()).toHaveLength(5);
  });
});

describe("keyboard", () => {
  it("Delete or Backspace on a focused row deletes it with Undo, and focus moves on", async () => {
    await start();
    const r = row("Question 2?");
    r.focus();
    press(r, "Delete");
    await tick();
    expect(shown()).not.toContain("Question 2?");
    expect(document.activeElement?.querySelector(".q")?.textContent).toBe("Question 3?");
    press(document.activeElement as HTMLElement, "Backspace");
    await tick();
    expect(shown()).toEqual(["Question 1?", "Question 4?", "Question 5?"]);
    expect(undo()).not.toBeNull();
  });

  it("Backspace on a picked row deletes every picked row; arrows move between rows", async () => {
    await start();
    const first = row("Question 1?");
    first.focus();
    press(first, "ArrowDown");
    expect(document.activeElement).toBe(row("Question 2?"));
    clickWith(row("Question 2?"), { metaKey: true });
    clickWith(row("Question 3?"), { metaKey: true });
    await tick();
    press(row("Question 3?"), "Backspace");
    await tick();
    expect(shown()).toEqual(["Question 1?", "Question 4?", "Question 5?"]);
  });

  it("every control in the list, the toast and the header has a name", async () => {
    await start();
    row("Question 1?").click();
    await tick();
    byLabel("Delete Question 2?")!.click();
    await tick();
    const unnamed = [...root.querySelectorAll<HTMLElement>("aside button, aside input, .toast button, .pane .actions button")].filter(
      (b) => !(b.getAttribute("aria-label") || b.textContent?.trim()),
    );
    expect(unnamed).toEqual([]);
    for (const li of root.querySelectorAll(".history li")) expect(li.getAttribute("aria-label")).toMatch(/\?, /);
    byLabel("Select roundtables to delete")!.click();
    await tick();
    for (const c of root.querySelectorAll(".row-check")) expect(c.getAttribute("aria-label")).toMatch(/^Select Question/);
  });
});

describe("clear all", () => {
  it("asks once with the count, then deletes everything", async () => {
    const { app, api } = await start("settings", rows(14));
    const b = [...root.querySelectorAll("button")].find((x) => x.textContent === "Clear all history")!;
    expect(b.getAttribute("aria-label")).toBe("Clear all history: delete 14 roundtables");
    expect(root.textContent).toContain("Add a Roundtable credit line to exports");
    const ask = vi.spyOn(window, "confirm").mockReturnValueOnce(false).mockReturnValueOnce(true);
    b.click();
    await tick();
    expect(ask).toHaveBeenCalledWith("Delete all 14 roundtables? This cannot be undone.");
    expect(api.calls).not.toContain("historyClear");
    b.click();
    await tick();
    expect(api.calls).toContain("historyClear");
    expect(app.state.history).toEqual([]);
    expect(await api.historyList()).toEqual([]);
    const again = [...root.querySelectorAll<HTMLButtonElement>("button")].find((x) => x.textContent === "Clear all history")!;
    expect(again.disabled).toBe(true);
  });

  it("says one roundtable, not 1 roundtables", () => {
    expect(clearAllQuestion(1)).toBe("Delete all 1 roundtable? This cannot be undone.");
  });
});

describe("what never becomes a roundtable", () => {
  it("a flag or an empty question is not a question", () => {
    for (const q of ["--help", "-h", "--version", "-V", "", "   ", "--help --json"]) expect(flagOnly(q), q).toBe(true);
    for (const q of ["Should we hire?", "-- is this a question?", "Is -h a flag?"]) expect(flagOnly(q), q).toBe(false);
  });

  it("the Ask button stays off for --help, and no run starts", async () => {
    const { app, api } = await start("main", []);
    app.state.ask = { ...app.state.ask, coach: false, question: "--help" };
    app.render();
    const go = [...root.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Ask the table")!;
    expect(go.disabled).toBe(true);
    await app.startAsk();
    await app.ask("--version", "quick", false);
    expect(api.calls.some((c) => c.startsWith("askStart"))).toBe(false);
  });

  it("the first-run sample is the setup check", () => {
    expect(runSource("onboarding", FIRST_QUESTION)).toBe("setup");
    expect(runSource("onboarding", "My own question?")).toBeNull();
    expect(runSource("main", FIRST_QUESTION)).toBeNull();
  });

  it("picks: toggle, range from the anchor, and what a delete takes", () => {
    const l = rows(5);
    expect(pickRows(l, [], null, "999-a1", "toggle")).toEqual(["999-a1"]);
    expect(pickRows(l, ["999-a1"], null, "999-a1", "toggle")).toEqual([]);
    expect(pickRows(l, [], "1000-a0", "997-a3", "range")).toEqual(["1000-a0", "999-a1", "998-a2", "997-a3"]);
    expect(deleteTargets(["1000-a0", "999-a1"], "999-a1")).toEqual(["1000-a0", "999-a1"]);
    expect(deleteTargets(["1000-a0", "999-a1"], "996-a4")).toEqual(["996-a4"]);
  });
});
