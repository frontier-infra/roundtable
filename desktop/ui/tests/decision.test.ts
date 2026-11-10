import { describe, expect, it } from "vitest";
import { applyEvent, defaultSettings, newRun } from "../src/state";
import { decisionPanel, type DecisionEvent } from "../src/decision";

const rated: DecisionEvent = {
  event: "decision",
  status: "rated",
  provider: "Jebadiah",
  model: "frontier-infra/jebadiah-9b-v2",
  endpoint: "http://127.0.0.1:8100/v1/systemone",
  options: [
    { key: "A", text: "Use SQLite" },
    { key: "B", text: "Use Postgres" },
    { key: "more_information", text: "Need more information" },
  ],
  probabilities: [0.62, 0.31, 0.07],
  confidence: 0.5,
  top: "A",
  table_pick: "B",
  disagrees: true,
  line: "Decision model: Jebadiah rates option A 0.62, B 0.31, more information 0.07. Confidence 0.50.",
  disagreement: "The decision model disagrees with the table's consensus: the table chose option B, but Jebadiah rates option A highest (0.62).",
};

describe("decision model", () => {
  it("is off by default", () => {
    expect(defaultSettings().decision?.provider).toBeNull();
  });

  it("folds the decision event into the run", () => {
    const r = applyEvent(newRun("Q", ["claude"], "talk"), rated);
    expect(r.decision).toEqual(rated);
  });

  it("shows the line, one bar per option, and the disagreement", () => {
    const el = decisionPanel(rated)!;
    expect(el.querySelector(".decision-line")!.textContent).toBe(rated.line);
    expect(el.querySelectorAll(".decision-bars li").length).toBe(3);
    expect(el.querySelector("li.top .label")!.textContent).toBe("Option A 0.62");
    expect(el.querySelector(".decision-warn")!.textContent).toContain("disagrees with the table");
    expect(el.textContent).not.toContain("\u2014");
  });

  it("says plainly when the model did not answer, and shows nothing when off", () => {
    const el = decisionPanel({ event: "decision", status: "unavailable", provider: "Jev", message: "Add your Jev key (TypeSafe) in Settings, or set TYPESAFE_API_KEY." })!;
    expect(el.textContent).toBe("Decision model: Jev did not answer. Add your Jev key (TypeSafe) in Settings, or set TYPESAFE_API_KEY.");
    expect(decisionPanel(undefined)).toBeNull();
  });
});

describe("decision model in the Markdown export", () => {
  it("adds the section after the verdict", async () => {
    const { runMarkdown } = await import("../src/session");
    const run = { ...applyEvent(newRun("Which database?", ["claude"], "talk"), rated) };
    const md = runMarkdown(run, { date: null });
    expect(md).toContain("## Decision model\n\nDecision model: Jebadiah rates option A 0.62, B 0.31, more information 0.07. Confidence 0.50.");
    expect(md).toContain("- **Option B** (0.31): Use Postgres");
    expect(md).toContain("**The decision model disagrees with the table's consensus");
  });
});
