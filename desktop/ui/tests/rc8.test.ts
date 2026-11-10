// rc8 (the usefulness bench fixes): the verdict leads and the seat answers fold under it,
// the chair's open questions when key facts are missing, and seat sections that skip the verdict.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi } from "../src/mock";
import { applyEvent, firstSection, newRun, replaceSection } from "../src/state";
import { formatVerdictBody } from "../src/session";

let root: HTMLElement;

beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
});

const tick = async (n = 12) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};

async function start() {
  const api = mockApi({ delay: 0, keys: ["claude", "gemini"], onboarded: true });
  const app = new App(api, root);
  await app.start({ view: "main" });
  await tick();
  return { app, api };
}

describe("the verdict leads", () => {
  it("folds the seat answers under the verdict and keeps the fold open once opened", async () => {
    const { app } = await start();
    app.state.ask = { ...app.state.ask, coach: false, verdict: "decision" };
    await app.askTable("Bread or garden?", null);
    await tick(30);
    const verdict = root.querySelector(".final-verdict")!;
    const fold = root.querySelector<HTMLDetailsElement>("details.seat-fold")!;
    expect(verdict).toBeTruthy();
    expect(fold).toBeTruthy();
    expect(verdict.compareDocumentPosition(fold) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(fold.open).toBe(false);
    expect(fold.querySelector(".answers")).toBeTruthy();
    expect(fold.querySelector("summary")!.textContent).toContain("2 seats");
    // The decision core reads as its own lines.
    expect(verdict.textContent).toContain("Would change it:");
    expect(verdict.textContent).toContain("Next steps:");
    expect(verdict.querySelectorAll("ol li").length).toBe(3);
    fold.open = true;
    fold.dispatchEvent(new Event("toggle"));
    app.set({ toast: "redraw" });
    await tick();
    expect(root.querySelector<HTMLDetailsElement>("details.seat-fold")!.open).toBe(true);
  });

  it("shows the seat answers open while the table answers, before any verdict", () => {
    let r = newRun("Bread or garden?", ["claude", "gemini"], 1);
    r = applyEvent(r, { event: "head_done", head: "claude", round: 1, status: "answered", text: "Garden." });
    expect(r.verdict).toBeUndefined();
  });

  it("puts the decision core on separate lines", () => {
    const out = formatVerdictBody("QUESTIONS FIRST:\n1. What is the workload?\nDECISION: Postgres.\nWOULD CHANGE IT: Heavy writes.\nNEXT STEPS:\n1. Count queries.");
    expect(out).toContain("**Questions first:**");
    expect(out).toContain("**Would change it:** Heavy writes.");
    expect(out).toContain("**Next steps:**\n\n1. Count queries.");
  });
});

describe("clarify first", () => {
  it("keeps the chair's open questions on the run", () => {
    let r = newRun("Which database?", ["claude", "gemini"], 2);
    r = applyEvent(r, { event: "clarify", questions: ["What is the workload?", "Which cloud?"] });
    expect(r.clarify).toEqual(["What is the workload?", "Which cloud?"]);
  });

  it("shows what the chair asks first while the table answers", async () => {
    const { app } = await start();
    app.state.ask = { ...app.state.ask, coach: false, verdict: "decision" };
    const run = app.askTable("Which database should we use?", null);
    await tick(3);
    const card = root.querySelector(".clarify-card");
    await run;
    await tick(30);
    expect(card?.textContent).toContain("The chair asks first");
    expect(card?.querySelectorAll("li").length).toBe(2);
    // Once the verdict lands it carries the questions; the card goes.
    expect(root.querySelector(".clarify-card")).toBeNull();
  });
});

describe("seat sections skip the brief and the verdict", () => {
  const t = [
    "# Round Table",
    "",
    "_Question:_ Q",
    "",
    "## Brief",
    "",
    "- **Goal:** g",
    "",
    "---",
    "## \u{1fa91} Verdict (decision)",
    "",
    "DECISION: A.",
    "",
    "---",
    "<details><summary>The seat answers (2 seats)</summary>",
    "",
    "---",
    "## Claude",
    "",
    "Garden.",
    "",
    "---",
    "## Gemini",
    "",
    "_(timed out)_",
    "",
    "</details>",
    "",
    "---",
    "_Advisory mode._",
    "",
  ].join("\n");

  it("swaps a retried seat in place and keeps the fold closed", () => {
    const retry = "# Round Table\n\n_Question:_ Q\n\n---\n## Gemini\n\nBread.\n\n---\n_Advisory mode._\n";
    const out = replaceSection(t, 1, firstSection(retry));
    expect(out).toContain("## Gemini\n\nBread.\n\n</details>");
    expect(out).toContain("## Claude\n\nGarden.");
    expect(out).toContain("DECISION: A.");
    expect(out).not.toContain("timed out");
  });
});
