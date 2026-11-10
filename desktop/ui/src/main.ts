import { App } from "./app";
import { tauriApi, type Api } from "./api";
import { mockApi } from "./mock";

// The browser preview (no Tauri) runs on the mock backend.
// Query: ?screen=1..5, ?view=main|settings, ?claude=none|app|code|both|plugin, ?keys=claude,gemini, ?os=windows,
// ?clis=some|none (AI tools found on the computer)
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
      history: q.get("history") === "1" ? sampleHistory() : [],
      clis: q.get("clis") === "none" ? "none" : "some",
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
    if (run === "talk") void app.ask("Should we move the product launch to March or keep February?", "talk", true);
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

void boot();
