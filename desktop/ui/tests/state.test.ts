import { describe, expect, it } from "vitest";
import {
  DEFAULT_ROUNDS,
  applyEvent,
  allFailed,
  back,
  canContinue,
  canSkip,
  emptyKeys,
  estimateLine,
  firstFailure,
  helpUrl,
  initialState,
  keyCardFromResult,
  keysFromStatus,
  looksLikeKey,
  newRun,
  next,
  requestEstimate,
  setupFailedMessage,
  splitVerdict,
  statusDot,
  tableHeads,
  defaultSettings,
  defaultCliHeads,
  cliHeadsOn,
  cliCardText,
  connectedHeads,
} from "../src/state";
import { sampleClis } from "../src/mock";

describe("walkthrough steps", () => {
  it("moves between 1 and 5 and stops at the ends", () => {
    expect(next(1)).toBe(2);
    expect(next(5)).toBe(5);
    expect(back(1)).toBe(1);
    expect(back(3)).toBe(2);
  });

  it("offers Skip for now on screens 3 to 5 only", () => {
    expect([1, 2, 3, 4, 5].map((s) => canSkip(s as 1))).toEqual([false, false, true, true, true]);
  });

  it("needs a finished setup on screen 2", () => {
    const s = initialState();
    s.step = 2;
    expect(canContinue(s)).toBe(false);
    s.setup = { status: "working", ticks: 1, attempts: 1 };
    expect(canContinue(s)).toBe(false);
    s.setup = { status: "done", ticks: 3, attempts: 1 };
    expect(canContinue(s)).toBe(true);
  });

  it("needs one connected AI on screen 3", () => {
    const s = initialState();
    s.step = 3;
    expect(canContinue(s)).toBe(false);
    s.keys.gemini = { status: "error", message: "x" };
    expect(canContinue(s)).toBe(false);
    s.keys.gemini = { status: "connected" };
    expect(canContinue(s)).toBe(true);
  });

  it("says twice-failed setup should go to Get help", () => {
    expect(setupFailedMessage(1)).toContain("Try again");
    expect(setupFailedMessage(2)).toContain("Get help");
  });
});

describe("keys", () => {
  it("names the right key store per OS", () => {
    expect(keyCardFromResult({ ok: true }, "macos").message).toBe("Connected. Saved in your Mac's Keychain.");
    expect(keyCardFromResult({ ok: true }, "windows").message).toBe("Connected. Saved in Windows Credential Manager.");
  });

  it("passes the engine's plain error through", () => {
    const c = keyCardFromResult({ ok: false, reason: "no_credit", message: "That account has no credit left. Add credit on the provider's site." }, "macos");
    expect(c.status).toBe("error");
    expect(c.message).toContain("no credit left");
  });

  it("reads status rows, including keys from the command-line version", () => {
    const k = keysFromStatus(
      { heads: { claude: { provider: "anthropic", has_key: true, source: "keychain" }, openai: { provider: "openai", has_key: true, source: "file" } }, importable: ["openai"] },
      "macos",
      emptyKeys(),
    );
    expect(k.claude.status).toBe("connected");
    expect(k.openai.message).toContain("command-line");
    expect(k.gemini.status).toBe("empty");
  });

  it("only checks things that look like a key", () => {
    expect(looksLikeKey("sk-ant-api03-abc")).toBe(true);
    expect(looksLikeKey("  \"AIzaSyD-1234567\"  ")).toBe(true);
    expect(looksLikeKey("short")).toBe(false);
    expect(looksLikeKey("two words here")).toBe(false);
  });
});

describe("asking", () => {
  it("counts requests honestly, multiplied by rounds", () => {
    expect(requestEstimate(3, 1)).toBe(3);
    expect(requestEstimate(3, "quick")).toBe(3);
    // Each round asks every seat, and the chair once.
    expect(requestEstimate(3, 2)).toBe(8);
    expect(requestEstimate(3, 3)).toBe(12);
    expect(requestEstimate(3, 4)).toBe(16);
    expect(requestEstimate(3, 9)).toBe(16);
    expect(estimateLine(3, 1)).toBe("This will ask 3 AIs, about 3 requests.");
    expect(estimateLine(1, 1)).toBe("This will ask 1 AI, about 1 request.");
    expect(estimateLine(2, 3)).toBe("This will ask 2 AIs, 3 rounds, at most 9 requests.");
    expect(estimateLine(2, 2, 4)).toBe("This will ask 2 AIs in 4 seats, 2 rounds, at most 10 requests.");
    expect(estimateLine(0, 2)).toContain("Connect an AI");
    expect(DEFAULT_ROUNDS).toBe(1);
    expect(initialState().ask.rounds).toBe(1);
    expect(initialState().ask.depth).toBe("quick");
  });

  it("keeps every round as a step, keyed by seat", () => {
    const seats = [
      { seat: "cfo@claude", head: "claude", persona: "CFO" },
      { seat: "nvidia-view@claude", head: "claude", persona: "The NVIDIA view", label: "Simulated perspective drawn from public positions. Not affiliated with or endorsed by NVIDIA." },
    ];
    let r = newRun("Q", ["claude"], 3, false, null, seats);
    expect(r.heads).toEqual(["cfo@claude", "nvidia-view@claude"]);
    r = applyEvent(r, { event: "run_started", heads: ["claude"], rounds: 3, mode: "deliberation", seats });
    for (const round of [1, 2, 3]) {
      for (const s of seats) {
        r = applyEvent(r, { event: "head_started", head: "claude", round, seat: s.seat });
        r = applyEvent(r, { event: "head_done", head: "claude", round, seat: s.seat, status: "answered", text: `${s.persona} r${round}` });
      }
      r = applyEvent(r, { event: "chair_verdict", round, consensus: round === 3, text: round === 3 ? "VERDICT: CONSENSUS\n- ok" : "VERDICT: CONTINUE\n- no" });
    }
    expect(r.steps.map((s) => s.round)).toEqual([1, 2, 3]);
    expect(r.steps[1].answers["cfo@claude"].text).toBe("CFO r2");
    expect(r.steps[2].verdict?.consensus).toBe(true);
    expect(r.cards["nvidia-view@claude"].text).toBe("The NVIDIA view r3");
    expect(r.cards["nvidia-view@claude"].label).toContain("Not affiliated with or endorsed by NVIDIA.");
    expect(r.cards["cfo@claude"].persona).toBe("CFO");
  });

  it("folds engine events into live cards", () => {
    let r = newRun("Q", ["claude", "gemini"], 3);
    r = applyEvent(r, { event: "run_started", heads: ["claude", "gemini"], rounds: 3, mode: "deliberation" });
    r = applyEvent(r, { event: "head_started", head: "claude", round: 1 });
    expect(r.cards.claude.status).toBe("thinking");
    r = applyEvent(r, { event: "head_done", head: "claude", round: 1, status: "answered", text: "Garden" });
    r = applyEvent(r, { event: "head_done", head: "gemini", round: 1, status: "failed", text: "", message: "That key did not work." });
    expect(r.cards.claude.text).toBe("Garden");
    expect(r.cards.gemini.status).toBe("failed");
    // Round 2 keeps the old answer visible while it revises.
    r = applyEvent(r, { event: "head_started", head: "claude", round: 2 });
    expect(r.cards.claude.text).toBe("Garden");
    expect(r.round).toBe(2);
    r = applyEvent(r, { event: "chair_verdict", round: 2, consensus: true, text: "VERDICT: CONSENSUS\n- garden" });
    r = applyEvent(r, { event: "run_done", transcript: "T", answered: 1, consensus: true });
    expect(r.done).toBe(true);
    expect(r.verdicts).toHaveLength(1);
    expect(allFailed(r)).toBe(false);
  });

  it("reports the first failure when every AI fails", () => {
    let r = newRun("Q", ["claude"], "quick");
    r = applyEvent(r, { event: "head_done", head: "claude", round: 1, status: "failed", text: "", message: "That account has no credit left." });
    r = applyEvent(r, { event: "process_exit" });
    expect(allFailed(r)).toBe(true);
    expect(firstFailure(r)).toBe("That account has no credit left.");
  });

  it("splits the chair's verdict line", () => {
    expect(splitVerdict("VERDICT: CONSENSUS\n**AGREED**\n- a")).toEqual({ agree: true, body: "**AGREED**\n- a" });
    expect(splitVerdict("**VERDICT: CONTINUE**\nx").agree).toBe(false);
    expect(splitVerdict("no verdict").body).toBe("no verdict");
  });

  it("seats the saved choice, limited to connected AIs", () => {
    const keys = emptyKeys();
    keys.claude = { status: "connected" };
    keys.gemini = { status: "connected" };
    expect(tableHeads(defaultSettings(), keys)).toEqual(["claude", "gemini"]);
    expect(tableHeads({ ...defaultSettings(), default_heads: ["gemini", "openai"] }, keys)).toEqual(["gemini"]);
    expect(tableHeads({ ...defaultSettings(), default_heads: ["openai"] }, keys)).toEqual(["claude", "gemini"]);
  });

  it("shows a green dot only when the engine runs and an AI is ready", () => {
    const keys = emptyKeys();
    expect(statusDot({ engineReady: true, keys })).toBe("warn");
    keys.claude = { status: "connected" };
    expect(statusDot({ engineReady: false, keys })).toBe("warn");
    expect(statusDot({ engineReady: true, keys })).toBe("ok");
  });

  it("fills the help link with the error code", () => {
    const u = helpUrl("https://example.test/new", "E_SELFTEST", "windows", "0.1.1");
    expect(u).toContain("title=Setup%20problem%20E_SELFTEST");
    expect(decodeURIComponent(u)).toContain("Error code: E_SELFTEST");
  });
});

describe("AI tools on this computer", () => {
  it("ticks found and signed-in tools by default, in table order", () => {
    expect(defaultCliHeads(sampleClis())).toEqual(["claude", "codex", "gemini"]);
  });

  it("uses the saved ticks before detection, then only tools still found and not signed out", () => {
    const s = initialState();
    s.settings = { ...defaultSettings(), cli_heads: ["grok", "claude", "qwen"] };
    expect(cliHeadsOn(s)).toEqual(["claude", "grok", "qwen"]);
    s.clis = { loaded: true, loading: false, list: sampleClis(), signingIn: [] };
    expect(cliHeadsOn(s)).toEqual(["claude"]);
  });

  it("joins tools and keys without repeats", () => {
    const keys = emptyKeys();
    keys.claude = { status: "connected" };
    keys.glm = { status: "connected" };
    expect(connectedHeads(keys, ["codex", "claude"])).toEqual(["claude", "codex", "glm"]);
    expect(tableHeads(defaultSettings(), emptyKeys(), ["gemini"])).toEqual(["gemini"]);
  });

  it("lets Continue on screen 3 with only a ticked tool", () => {
    const s = initialState();
    s.step = 3;
    expect(canContinue(s)).toBe(false);
    s.settings = { ...defaultSettings(), cli_heads: ["codex"] };
    s.clis = { loaded: true, loading: false, list: sampleClis(), signingIn: [] };
    expect(canContinue(s)).toBe(true);
  });

  it("words each card plainly", () => {
    const [claude, , , grok] = sampleClis();
    expect(cliCardText(claude, "macos")).toBe("Claude Code, signed in. Uses your Claude plan.");
    expect(cliCardText(grok, "macos")).toBe("Grok is installed but not signed in. Click Sign in to open it in Terminal.");
    expect(cliCardText({ ...grok, signed_in: null }, "macos")).toBe("Grok is installed. We could not tell if you are signed in.");
  });
});
