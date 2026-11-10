// rc7: the question coach, suggested seats, a blind or open first round, verdict shapes with the
// minority view, Copy as Markdown and Download .md, follow-ups to the table or one seat, files and
// links every seat reads, and the session export.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, type MockOptions } from "../src/mock";
import { applyEvent, newRun, type RunView } from "../src/state";
import {
  attachmentsBlock,
  coachText,
  formatVerdictBody,
  briefMarkdown,
  mdFileName,
  priorText,
  runMarkdown,
  seatPickOf,
  sessionMarkdown,
  splitFinalVerdict,
} from "../src/session";

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
const type = async (sel: string, value: string, i = 0) => {
  const el = root.querySelectorAll<HTMLTextAreaElement | HTMLInputElement>(sel)[i];
  if (!el) throw new Error(`no ${sel}`);
  el.value = value;
  el.dispatchEvent(new Event("input"));
  await tick(2);
};

/** A finished two-seat, two-round run with a vote verdict. */
function sampleRun(): RunView {
  let r = newRun("Bread or garden?", ["claude", "gemini"], 2, false, null, [
    { seat: "cfo@claude", head: "claude", persona: "CFO" },
    { seat: "gemini", head: "gemini" },
  ]);
  r = { ...r, brief: "- **Goal:** Pick one.\n- **Constraints:** One day.", verdictShape: "vote", attachments: ["plan.pdf"] };
  const ev = (e: Parameters<typeof applyEvent>[1]) => (r = applyEvent(r, e));
  ev({ event: "head_done", head: "claude", seat: "cfo@claude", round: 1, status: "answered", text: "Garden: it pays off." });
  ev({ event: "head_done", head: "gemini", seat: "gemini", round: 1, status: "answered", text: "Bread: a result today." });
  ev({ event: "chair_verdict", round: 1, consensus: false, text: "VERDICT: CONTINUE\n**AGREED**\n- Both cheap." });
  ev({ event: "head_done", head: "claude", seat: "cfo@claude", round: 2, status: "answered", text: "POSITION: Garden." });
  ev({ event: "head_done", head: "gemini", seat: "gemini", round: 2, status: "answered", text: "POSITION: Still bread." });
  ev({ event: "verdict", shape: "vote", text: "**VOTES**\n- CFO: garden\n- Gemini: bread\nRESULT: tie\nMINORITY VIEW: Bread first.\nHELD BY: Gemini", minority: "Bread first.", held_by: "Gemini" });
  ev({ event: "run_done", transcript: "# t", answered: 2, consensus: false });
  return r;
}

describe("brief, material and verdict text", () => {
  it("writes the brief in the engine's shape", () => {
    expect(briefMarkdown({ goal: " Pick a  database. ", context: "A small service.", constraints: "", good_answer: "One pick, with the risk." })).toBe(
      "- **Goal:** Pick a database.\n- **Context:** A small service.\n- **A good answer:** One pick, with the risk.",
    );
  });

  it("shares material in the engine's block", () => {
    const block = attachmentsBlock([
      { name: "a.md", kind: "text", chars: 5, shared: "full", text: "short\n" },
      { name: "b.pdf", kind: "pdf", chars: 40000, shared: "summary", text: "- the gist" },
      { name: "c.txt", kind: "text", chars: 40000, shared: "start", text: "start" },
    ]);
    expect(block).toBe("Every seat reads the same material.\n\n### a.md\nshort\n\n### b.pdf (summary)\n- the gist\n\n### c.txt (first 12000 characters)\nstart");
    expect(attachmentsBlock([])).toBe("");
  });

  it("splits the minority view out of the verdict", () => {
    const v = splitFinalVerdict("DECISION: Garden.\nCONFIDENCE: High (80%)\n**MINORITY VIEW:** Bread first.\nHELD BY: Gemini");
    expect(v).toEqual({ body: "DECISION: Garden.\nCONFIDENCE: High (80%)", minority: "Bread first.", heldBy: "Gemini" });
    expect(splitFinalVerdict("RESULT: tie\nMINORITY VIEW: None\nHELD BY: Nobody")).toEqual({ body: "RESULT: tie", minority: undefined, heldBy: undefined });
  });

  it("reads seat keys back into seats", () => {
    expect(seatPickOf("cfo@claude#2")).toEqual({ persona: "cfo", head: "claude" });
    expect(seatPickOf("gemini")).toEqual({ persona: "", head: "gemini" });
  });

  it("puts each verdict key line on its own", () => {
    expect(formatVerdictBody("DECISION: Garden.\nCONFIDENCE: High (80%)\n**WHY**\n- a")).toBe("**Decision:** Garden.\n\n**Confidence:** High (80%)\n\n**WHY**\n- a");
  });
});

describe("Markdown export", () => {
  it("holds the brief, the seats (persona and AI), each round and the verdict with the minority", () => {
    const md = runMarkdown(sampleRun(), { date: null });
    expect(md.startsWith("# Roundtable: Bread or garden?\n\n**Mode:** Quick, up to 2 rounds\n\n## Brief\n\n- **Goal:** Pick one.")).toBe(true);
    expect(md).toContain("## Seats\n\n| Seat | Persona | AI |\n|---|---|---|\n| 1 | CFO | Claude |\n| 2 | Generic | Gemini |");
    expect(md).toContain("## Material every seat read\n\n- plan.pdf");
    expect(md).toContain("## Round 1: each seat answered on its own\n\n### CFO on Claude\n\nGarden: it pays off.");
    expect(md).toContain("### Chair after round 1: they still differ\n\n**AGREED**");
    expect(md).toContain("## Round 2: they read each other and revised\n\n### CFO on Claude\n\nGarden.");
    expect(md).toContain("## Verdict: Vote\n\n**VOTES**");
    expect(md).toContain("**Result:** tie");
    expect(md).toContain("**Minority view:** Bread first.\n\n**Held by:** Gemini");
    expect(md).not.toContain("MINORITY VIEW:");
    expect(md).not.toContain("—");
  });

  it("exports a session of turns, and names files", () => {
    const a = sampleRun();
    const b = { ...sampleRun(), question: "And if it rains?", target: "gemini" };
    const md = sessionMarkdown([a, b], new Date(0));
    expect(md.startsWith("# Roundtable session: Bread or garden?\n\n_2 turns.")).toBe(true);
    expect(md).toContain("## Turn 1\n\n**Question:** Bread or garden?");
    expect(md).toContain("## Turn 2: follow-up\n\n**Question:** And if it rains?\n\n_Asked of one seat: Gemini._");
    expect(md).toContain("### Brief");
    expect(mdFileName("Should we raise prices 20%?")).toBe("roundtable-should-we-raise-prices-20.md");
    expect(mdFileName("x", true)).toBe("roundtable-session-x.md");
  });

  it("carries earlier turns into a follow-up", () => {
    const p = priorText([sampleRun()]);
    expect(p).toContain("### Turn 1: Bread or garden?");
    expect(p).toContain("- **CFO on Claude:** Garden.");
    expect(p).toContain("Verdict (vote):");
  });
});

describe("the question coach", () => {
  it("asks, writes an editable brief, suggests seats, then sends the brief to the table", async () => {
    const { app, api } = await start({ clis: "none" });
    await type("textarea.question", "Bread or garden?");
    await click("Sharpen and ask");
    expect(api.calls).toContain("coachQuestions:Bread or garden?");
    expect(text()).toContain("2 questions before the table meets");
    await type(".coach-qa textarea", "Something that lasts", 0);
    await type(".coach-qa textarea", "About 50 dollars", 1);
    await click("Write the brief");
    expect(api.calls).toContain("coachBrief:Something that lasts|About 50 dollars");
    expect(text()).toContain("The brief the table will read");
    expect(api.calls.some((c) => c.startsWith("suggestPanel:claude,gemini"))).toBe(true);
    expect(text()).toContain("Suggested seats");
    expect(root.querySelectorAll(".panel .seat-pick").length).toBe(3);
    // Edit the brief, then go.
    await type('.brief-field[data-field="constraints"] textarea', "Under 50 dollars");
    await click("Ask the table");
    await tick(30);
    const brief = api.calls.find((c) => c.startsWith("brief:"))!;
    expect(brief).toContain("- **Constraints:** Under 50 dollars");
    expect(brief).toContain("- **Context:** Something that lasts About 50 dollars");
    expect(api.calls.find((c) => c.startsWith("askStart:"))).toBe("askStart:claude,gemini:1:cfo@claude,head-of-product@gemini,customer-advocate@claude");
    expect(api.calls).toContain("verdict:decision");
    expect(app.state.run?.brief).toContain("Under 50 dollars");
    expect(text()).toContain("The brief");
  });

  it("keeps my seats when I choose them over the suggestion", async () => {
    const { api } = await start();
    await type("textarea.question", "Q?");
    await click("Sharpen and ask");
    await click("Write the brief");
    await click("My seats");
    await click("Ask the table");
    await tick(20);
    expect(api.calls.find((c) => c.startsWith("askStart:"))).toBe("askStart:claude,gemini:1");
  });

  it("can be skipped, or turned off, and says so when it cannot answer", async () => {
    const { app, api } = await start({ coachFails: true });
    await type("textarea.question", "Q?");
    await click("Sharpen and ask");
    expect(text()).toContain("The coach is not available");
    await click("Ask the table as typed");
    await tick(20);
    expect(api.calls.some((c) => c.startsWith("brief:"))).toBe(false);
    expect(app.state.run?.question).toBe("Q?");
    app.newQuestion();
    await tick();
    (root.querySelector("#t-sharpen-my-question-first") as HTMLInputElement).click();
    await tick();
    await type("textarea.question", "Plain?");
    await click("Ask the table");
    await tick(20);
    expect(api.calls.filter((c) => c.startsWith("coachQuestions")).length).toBe(1);
  });

  it("goes straight to the brief when the question is already clear", async () => {
    const { api } = await start({ coachQuestions: [] });
    await type("textarea.question", "Clear?");
    await click("Sharpen and ask");
    expect(api.calls).toContain("coachBrief:");
    expect(text()).toContain("The brief the table will read");
  });
});

describe("blind first round", () => {
  it("is blind by default; the setting opens it", async () => {
    const { app, api } = await start();
    app.state.ask.coach = false;
    await app.askTable("One?", null);
    await tick(20);
    expect(api.calls).not.toContain("open-first-round");
    await app.saveSettings({ blind_first_round: false });
    app.newQuestion();
    await app.askTable("Two?", null);
    await tick(20);
    expect(api.calls).toContain("open-first-round");
    expect(text()).toContain("Round 1 was open");
  });
});

describe("verdicts", () => {
  it("shows the shape, the minority and who held it, with Copy as Markdown and Download .md", async () => {
    const { app, api } = await start();
    app.state.ask = { ...app.state.ask, coach: false, verdict: "vote" };
    await app.askTable("Bread or garden?", null);
    await tick(30);
    const card = root.querySelector(".final-verdict")!;
    expect(card.getAttribute("data-shape")).toBe("vote");
    expect(card.textContent).toContain("Vote");
    expect(card.textContent).toContain("Minority view");
    expect(card.textContent).toContain("Bake bread first");
    expect(card.textContent).toContain("Held by Gemini");
    const copy = [...card.querySelectorAll("button")].find((b) => b.textContent === "Copy as Markdown")!;
    copy.click();
    await tick();
    const copied = api.calls.find((c) => c.startsWith("copy:"));
    expect(copied).toBeTruthy();
    expect(app.state.toast).toBe("Copied as Markdown.");
    expect([...card.querySelectorAll("button")].some((b) => b.textContent === "Download .md")).toBe(true);
  });

  it("puts the decision's confidence on the card", async () => {
    const { app } = await start();
    app.state.ask = { ...app.state.ask, coach: false, verdict: "decision" };
    await app.askTable("Bread or garden?", null);
    await tick(30);
    expect(root.querySelector(".final-verdict .state.conf-medium")?.textContent).toBe("Medium confidence");
  });

  it("saves the shape as the next default", async () => {
    const { app } = await start();
    const sel = root.querySelector<HTMLSelectElement>(".verdict-select")!;
    sel.value = "plan";
    sel.dispatchEvent(new Event("change"));
    await tick();
    expect(app.state.settings.verdict_shape).toBe("plan");
    expect(app.state.ask.verdict).toBe("plan");
  });
});

describe("follow-ups and material", () => {
  it("asks the whole table or one seat, keeping the history, and exports the session", async () => {
    const { app, api } = await start();
    app.state.ask = { ...app.state.ask, coach: false };
    await app.askTable("Bread or garden?", null);
    await tick(30);
    expect(text()).toContain("Follow up");
    await type(".follow-up textarea", "What if it rains?");
    const target = root.querySelector<HTMLSelectElement>(".follow-target")!;
    expect([...target.options].map((o) => o.textContent)).toEqual(["The whole table", "Only Claude", "Only Gemini"]);
    target.value = "gemini";
    target.dispatchEvent(new Event("change"));
    await click("Ask");
    await tick(30);
    const starts = api.calls.filter((c) => c.startsWith("askStart:"));
    expect(starts[1]).toBe("askStart:gemini:1");
    expect(api.calls.some((c) => c.startsWith("prior:"))).toBe(true);
    expect(app.state.session.length).toBe(1);
    expect(app.state.run?.target).toBe("gemini");
    expect(text()).toContain("Follow-up to Gemini");
    expect(root.querySelectorAll(".turn").length).toBe(1);
    // Then the whole table again.
    await type(".follow-up textarea", "So which first?");
    await click("Ask");
    await tick(30);
    expect(api.calls.filter((c) => c.startsWith("askStart:"))[2]).toBe("askStart:claude,gemini:1");
    expect(app.state.session.length).toBe(2);
    await click("Export session");
    expect(app.state.toast).toBe("Session saved.");
  });

  it("reads files and links once and gives every seat the same copy", async () => {
    const { app, api } = await start();
    app.state.ask = { ...app.state.ask, coach: false };
    app.render();
    await click("Add a file");
    expect(api.calls).toContain("attachPrepare:/Users/someone/plan.pdf");
    expect(text()).toContain("Summarized once; every seat reads the summary.");
    await type(".link-input", "https://example.com/brief");
    await click("Add link");
    expect(api.calls).toContain("attachPrepare:https://example.com/brief");
    expect(root.querySelectorAll(".attachment").length).toBe(2);
    await type(".link-input", "not a link");
    await click("Add link");
    expect(app.state.toast).toContain("full link");
    await app.askTable("Use the plan?", null);
    await tick(30);
    const att = api.calls.find((c) => c.startsWith("attached:"))!;
    expect(att).toContain("Every seat reads the same material.");
    expect(att).toContain("### plan.pdf (summary)");
    expect(att).toContain("### https://example.com/brief");
    expect(text()).toContain("Every seat read: plan.pdf, example.com/brief.");
  });

  it("lets the coach read attached material, so it does not ask for it", async () => {
    const { app, api } = await start({ clis: "none" });
    await click("Add a file");
    await type("textarea.question", "Use the plan?");
    await click("Sharpen and ask");
    const q = api.calls.find((c) => c.startsWith("coachQuestions:"))!;
    expect(q).toContain("Use the plan?\n\nATTACHED MATERIAL (every seat already reads this; do not ask for it):\nEvery seat reads the same material.");
    expect(q).toContain("### plan.pdf (summary)");
    expect(app.state.coach?.question).toBe("Use the plan?");
    expect(coachText("Use the plan?", [])).toBe("Use the plan?");
  });

  it("says plainly when a file cannot be read", async () => {
    const { app } = await start();
    await app.addAttachment("/tmp/missing.md");
    expect(app.state.toast).toBe("Could not find that file.");
    expect(app.state.ask.attachments.length).toBe(0);
  });
});
