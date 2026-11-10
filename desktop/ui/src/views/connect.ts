// "Use Roundtable from your AI": Codex and Cursor in one click, copyable setup for any
// other MCP app, and the Share with ChatGPT switch with its call log.

import type { App } from "../app";
import type { ShareLogEntry } from "../api";
import { button, h, toggle } from "../dom";
import { glyph, headLogo } from "../brand";

function copyBox(app: App, text: string, label = "Copy"): HTMLElement {
  return h("div", { class: "copy-box" }, h("pre", { text }), button(label, () => void app.api.copyText(text).then(() => app.showToast("Copied.")), "btn small"));
}

function clientRow(app: App, id: "codex" | "cursor", name: string, logo: string): HTMLElement | null {
  const c = app.state.clients;
  const found = id === "codex" ? c.codex_found : c.cursor_found;
  const connected = id === "codex" ? c.codex_connected : c.cursor_connected;
  if (!found) return null;
  const msg = c.messages[id];
  return h(
    "div",
    { class: `connect-row${connected ? " done" : ""}`, attrs: { "data-target": id } },
    headLogo(logo),
    h("div", { class: "connect-text" }, h("strong", { text: `${name} found.` }), msg ? h("p", { class: "status ok", text: msg }) : connected ? h("p", { class: "status ok", text: "Connected." }) : null),
    connected
      ? h(
          "span",
          { class: "connect-done" },
          h("span", { class: "check", attrs: { role: "img", "aria-label": "Connected" } }, glyph("check")),
          button("Disconnect", () => void app.disconnectClient(id), "btn small ghost"),
        )
      : button(c.busy === id ? "Connecting..." : `Connect to ${name}`, () => void app.connectClient(id), "btn primary", !!c.busy),
  );
}

/** Codex, Cursor, and the snippets for every other MCP app. */
export function otherApps(app: App): HTMLElement[] {
  const c = app.state.clients;
  if (c.loading) return [h("p", { class: "muted", role: "status", text: "Looking for Codex and Cursor on this computer..." })];
  const out: HTMLElement[] = [];
  const rows = [clientRow(app, "codex", "Codex", "codex"), clientRow(app, "cursor", "Cursor", "cursor")].filter((r): r is HTMLElement => !!r);
  out.push(...rows);
  if (c.error) out.push(h("p", { class: "status bad", role: "alert", text: c.error }));
  const snippets = c.snippets ?? [];
  if (snippets.length) {
    out.push(
      h(
        "details",
        { class: "snippets" },
        h("summary", { text: "Another AI app? Copy its setup" }),
        ...snippets.map((s) => h("div", { class: "snippet", attrs: { "data-snippet": s.id } }, h("p", { class: "small" }, h("strong", { text: s.name }), ` ${s.where}`), copyBox(app, s.text))),
      ),
    );
  }
  return out;
}

function when(ts?: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/** One plain line per logged request. */
export function logLine(e: ShareLogEntry): string {
  const t = when(e.ts);
  const q = e.question ? `"${e.question.length > 80 ? e.question.slice(0, 79) + "..." : e.question}"` : "";
  switch (e.type) {
    case "started":
    case "stopped":
      return `${t} ${e.note ?? e.type}`;
    case "call_started":
      return `${t} Roundtable started: ${q}`;
    default: {
      if (e.status === 401) return `${t} Refused: wrong or missing token${e.peer ? ` (from ${e.peer})` : ""}.`;
      if (e.status === 429) return `${t} Refused: rate limit${q ? ` for ${q}` : ""}.`;
      if (e.status === 403) return `${t} Refused: ${e.note ?? "origin not allowed"}.`;
      if (e.method === "tools/call") return `${t} Roundtable ${e.is_error ? "failed" : "finished"}: ${q}${e.result ? `. ${e.result}` : ""}${e.ms ? ` (${Math.round(e.ms / 1000)} s)` : ""}`;
      return `${t} ${e.method || "request"}${e.status && e.status !== 200 ? ` (${e.status})` : ""}`;
    }
  }
}

/** The Share with ChatGPT switch: off by default, a fresh link each time, every call logged. */
export function shareWithChatGPT(app: App): HTMLElement[] {
  const v = app.state.share;
  const st = v.status;
  const on = !!st?.running;
  const out: HTMLElement[] = [
    toggle(
      "Share with ChatGPT",
      on,
      (x) => void app.setShare(x),
      "Gives Roundtable a private web link for ChatGPT, the OpenAI API and Claude connectors. Off ends the link. Calls use the AIs on this computer.",
    ),
  ];
  if (v.busy) out.push(h("p", { class: "muted", role: "status", text: on ? "Stopping..." : "Starting the link. This takes a few seconds..." }));
  if (v.error) out.push(h("p", { class: "status bad", role: "alert", text: v.error }));
  if (!on && st?.ended) out.push(h("p", { class: "status bad", text: st.ended }));
  if (!on && st && st.cloudflared === false && !v.error) out.push(h("p", { class: "small muted", text: `Needs cloudflared. ${st.install ?? ""}` }));
  if (on && st?.url) {
    out.push(
      h("p", { class: "small", text: "In ChatGPT on the web: Settings, Security and login, turn on Developer mode. Then open Plugins, press +, paste this link as the MCP server URL, and choose No authentication. The link holds the key, so share it with no one." }),
      copyBox(app, st.url, "Copy link"),
      h(
        "details",
        { class: "snippets" },
        h("summary", { text: "For the OpenAI or Claude API (URL and token apart)" }),
        h("p", { class: "small", text: "Server URL:" }),
        copyBox(app, st.server_url ?? ""),
        h("p", { class: "small", text: "Token (send as Authorization: Bearer):" }),
        copyBox(app, st.token ?? ""),
      ),
    );
  }
  const log = (st?.log ?? []).filter((e) => e.type !== "request" || e.method === "tools/call" || (e.status ?? 200) >= 400);
  if (log.length) {
    out.push(
      h("h3", { text: "Calls" }),
      h(
        "ol",
        { class: "share-log small", attrs: { "aria-live": "polite" } },
        ...log
          .slice(-30)
          .reverse()
          .map((e) => h("li", { class: (e.status ?? 200) >= 400 || e.is_error ? "bad" : "", text: logLine(e) })),
      ),
    );
  }
  return out;
}
