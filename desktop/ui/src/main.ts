import { App } from "./app";
import { tauriApi, type Api } from "./api";
import { mockApi } from "./mock";

// The browser preview (no Tauri) runs on the mock backend.
// Query: ?screen=1..5, ?view=main|settings, ?claude=none|app|code|both|plugin, ?keys=claude,gemini, ?os=windows,
// ?clis=some|none (AI tools found on the computer), ?providers=1 (Ollama found here and OpenRouter added with a key)
async function boot() {
  const root = document.getElementById("app")!;
  const q = new URLSearchParams(location.search);
  const inTauri = "__TAURI_INTERNALS__" in window;
  let api: Api;
  if (inTauri) {
    api = await tauriApi();
  } else {
    api = mockApi({
      os: q.get("os") ?? "macos",
      claude: q.get("claude") ?? "both",
      keys: (q.get("keys") ?? "").split(",").filter(Boolean),
      onboarded: q.get("view") === "main" || q.get("view") === "settings",
      setupFails: q.get("fail") === "1",
      delay: Number(q.get("delay") ?? 400),
      history: q.get("history") === "1" ? sampleHistory() : q.get("history") === "many" ? manyHistory() : [],
      clis: q.get("clis") === "none" ? "none" : "some",
      noLabPack: q.get("labpack") === "0",
      memoryGb: q.get("mem") ? Number(q.get("mem")) : undefined,
      arch: q.get("arch") ?? undefined,
      localProviders: q.get("providers") === "1" ? "ollama" : undefined,
      providers:
        q.get("providers") === "1"
          ? [{ id: "openrouter", preset: "openrouter", name: "OpenRouter", base_url: "https://openrouter.ai/api/v1", needs_key: true, models: [{ id: "openai/gpt-4o-mini", price_in: 0.15, price_out: 0.6 }] }]
          : undefined,
      providerKeys: q.get("providers") === "1" ? ["openrouter"] : undefined,
      // ?update=0.1.3 (a newer version on Stable), ?version=0.1.2 (this app's version)
      update: q.get("update") ? { version: q.get("update")!, notes: "### Fixed\n\n- A preview of the notes from the update manifest.", channel: q.get("channel") ?? "stable" } : undefined,
      version: q.get("version") ?? undefined,
      custom: q.get("custom") === "1" ? [{ id: "head-of-support", name: "Head of Support", expertise: "Customers after they buy", brief: "Weigh repeat contacts.", ai: "gemini" }] : [],
    });
  }
  const app = new App(api, root);
  const screen = !inTauri && q.get("screen") ? Number(q.get("screen")) : undefined;
  const view = !inTauri ? (q.get("view") as "main" | "settings" | null) ?? undefined : undefined;
  await app.start({ screen, view });
  if (!inTauri) {
    (window as unknown as { rt: App }).rt = app;
    // Preview only: ?run=setup|quick|talk plays that flow on the mock backend.
    const run = q.get("run");
    if (run === "setup") void app.runSetup();
    if (run === "quick") void app.ask(app.state.firstQuestion, "quick", false);
    // ?seats=preset:pricing|custom, ?rounds=1..4, ?persona=new (Settings editor), ?open=1 (open every round)
    const seats = q.get("seats");
    if (seats) app.set({ ask: { ...app.state.ask, seats, seatList: seats === "custom" ? [{ persona: "cfo", head: "claude" }, { persona: "nvidia-view", head: "gemini" }, { persona: "", head: "gemini" }] : [] } });
    if (q.get("rounds")) app.set({ ask: { ...app.state.ask, rounds: Number(q.get("rounds")) } });
    if (q.get("persona") === "new") app.set({ personaEdit: { id: "", name: "Head of Support", expertise: "Customers after they buy", brief: "Reason from the support queue. Weigh time to first answer and repeat contacts. Be skeptical of features nobody asked for.", ai: "gemini" } });
    if (run === "talk") void app.ask("Should we move the product launch to March or keep February?", Number(q.get("rounds") ?? 2), true);
    if (run === "ask") void app.askTable("Should we raise prices 20% for new customers?", null);
    // rc7 preview: ?verdict=decision|pros-cons|plan|vote, ?coach=1|brief, ?attach=1, ?follow=1, ?blind=0
    const verdict = q.get("verdict");
    if (verdict) app.set({ ask: { ...app.state.ask, verdict: verdict as typeof app.state.ask.verdict } });
    if (q.get("blind") === "0") void app.saveSettings({ blind_first_round: false });
    if (q.get("attach") === "1") {
      void app.addAttachment("/Users/someone/plan.pdf").then(() => app.addAttachment("https://example.com/pricing-notes"));
    }
    const coach = q.get("coach");
    if (coach) {
      app.set({ ask: { ...app.state.ask, question: "Should we raise prices 20% for new customers?" } });
      void app.openCoach("Should we raise prices 20% for new customers?").then(async () => {
        if (coach !== "brief") return;
        app.state.coach!.answers = ["Grow revenue without losing small teams.", "Under 3% churn."];
        await app.writeBrief();
      });
    }
    if (q.get("follow") === "1") {
      void app.askTable("Should we raise prices 20% for new customers?", null).then(() =>
        setTimeout(() => void app.askFollowUp("What if a competitor cuts prices next month?", app.state.run!.heads[1] ?? "table"), 4000),
      );
    }
    // ?whatsnew=update (the banner's notes, with ?update=) or all (Settings, What's new)
    const wn = q.get("whatsnew");
    if (wn) setTimeout(() => (wn === "all" ? app.showChangelog() : app.showUpdateNotes()), 300);
    if (q.get("open") === "1") setInterval(() => document.querySelectorAll("details.step").forEach((d) => d.setAttribute("open", "")), 200);
  }
}

function sampleHistory() {
  const now = Math.floor(Date.now() / 1000);
  return [
    { id: `${now - 600}-a1b2`, ts: now - 600, question: "Should we move the launch to March or keep February?", source: "claude", mode: "deliberation", heads: ["claude", "gemini", "openai"], consensus: true },
    { id: `${now - 90000}-c3d4`, ts: now - 90000, question: "Which laptop is better for a student: MacBook Air or ThinkPad X1?", source: "app", mode: "advisory", heads: ["claude", "gemini"] },
    { id: `${now - 400000}-e5f6`, ts: now - 400000, question: "Rewrite my cover letter opening so it sounds less stiff", source: "app", mode: "advisory", heads: ["claude", "openai"] },
  ];
}

/** ?history=many: a longer list, for the delete and select screens. */
function manyHistory() {
  const now = Math.floor(Date.now() / 1000);
  const qs = [
    "Should our 3-person IT shop hire a fourth person or buy a managed tool?",
    "What is the single biggest risk of Tauri for this team?",
    "Should our IT team build internal desktop tools with Tauri or Electron?",
    "Do we renew the Datto contract or move backups to Rosa?",
    "Which laptop is better for a student: MacBook Air or ThinkPad X1?",
    "Should we move the launch to March or keep February?",
    "Rewrite my cover letter opening so it sounds less stiff",
    "Is a 20% price rise for new customers safe this quarter?",
    "Which CRM should a five-person agency start with?",
    "Should we sign the two-year office lease or go month to month?",
    "What should the first onboarding email say?",
    "Can one Mac Studio replace the two render boxes?",
    "Should we offer a free tier or a 14-day trial?",
    "Where should the team offsite be this spring?",
  ];
  return qs.map((question, i) => {
    const ts = now - 600 - i * (i < 4 ? 1500 : 70000);
    return { id: `${ts}-${(0xa000 + i).toString(16)}`, ts, question, source: i === 5 ? "claude" : "app", mode: "advisory", heads: ["claude", "gemini"] };
  });
}

void boot();
