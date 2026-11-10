// Updates and What's new (spec section 9).
// The app checks the signed manifest at launch and every 6 hours. A newer version shows a quiet
// banner. The changelog ships inside the app (desktop/CHANGELOG.md), and the manifest carries
// the new version's section, so What's new works before and after an update.

import changelogText from "../../CHANGELOG.md?raw";
import type { App } from "./app";
import { button, h, toggle } from "./dom";
import { renderMarkdown } from "./markdown";

export const CHANGELOG = changelogText;

export interface ChangelogSection {
  version: string;
  date: string;
  body: string;
}

/** What the core found on the update channel. */
export interface UpdateView {
  version: string;
  notes: string;
  date: string | null;
  /** Downloaded and checked; installs on quit when automatic updates are on. */
  ready: boolean;
  busy: "downloading" | "installing" | null;
  /** Later was clicked for this version. */
  dismissed: boolean;
}

export interface UpdateStatus {
  checking: boolean;
  checkedAt: number | null;
  error: string | null;
}

export interface WhatsNew {
  title: string;
  sections: ChangelogSection[];
}

/** Sections, newest first. Same format the release scripts read. */
export function parseChangelog(text: string): ChangelogSection[] {
  const out: { version: string; date: string; lines: string[] }[] = [];
  for (const line of text.replace(/\r\n/g, "\n").split("\n")) {
    const m = line.match(/^## (\d+\.\d+\.\d+)(?:\s+-\s+(.+?))?\s*$/);
    if (m) out.push({ version: m[1], date: m[2] ?? "", lines: [] });
    else if (out.length) out[out.length - 1].lines.push(line);
  }
  return out.map((s) => ({ version: s.version, date: s.date, body: s.lines.join("\n").trim() }));
}

/** Semver order for our versions: 0.1.2-rc.1 < 0.1.2-rc.2 < 0.1.2 < 0.1.3. Unknown text sorts first. */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) => {
    const m = /^(\d+)\.(\d+)\.(\d+)(?:-rc\.(\d+))?$/.exec(v.trim());
    if (!m) return [-1, -1, -1, -1];
    return [Number(m[1]), Number(m[2]), Number(m[3]), m[4] === undefined ? Infinity : Number(m[4])];
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < 4; i++) if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
  return 0;
}

const base = (v: string) => v.replace(/-.*$/, "");

/**
 * What changed between two versions: every section after `from`, up to and including `to`.
 * A test build of the same release (0.1.2-rc.1 to rc.2) shows that release's section.
 */
export function sectionsBetween(sections: ChangelogSection[], from: string, to: string): ChangelogSection[] {
  const between = sections.filter((s) => compareVersions(s.version, base(from)) > 0 && compareVersions(s.version, base(to)) <= 0);
  if (between.length) return between;
  return sections.filter((s) => s.version === base(to));
}

/** After an update: What's new since the version that last showed it. null when nothing to show. */
export function whatsNewAfterUpdate(lastSeen: string, current: string, text = CHANGELOG): WhatsNew | null {
  if (!lastSeen || !current || compareVersions(current, lastSeen) <= 0) return null;
  const sections = sectionsBetween(parseChangelog(text), lastSeen, current);
  if (!sections.length) return null;
  return { title: `Roundtable is now ${current}`, sections };
}

function sectionBlock(s: ChangelogSection, showHead: boolean): HTMLElement {
  return h(
    "section",
    { class: "whatsnew-section" },
    showHead ? h("h3", null, `Roundtable ${s.version}`, s.date ? h("span", { class: "muted small", text: ` ${s.date}` }) : null) : null,
    renderMarkdown(s.body),
  );
}

/** The What's new sheet: after an update, from the banner, or from Settings. */
export function renderWhatsNew(app: App, w: WhatsNew): HTMLElement {
  const close = () => app.set({ whatsNew: null });
  const u = app.state.update;
  const forUpdate = !!u && w.sections.length === 1 && w.title.includes(u.version);
  return h(
    "div",
    {
      class: "sheet-backdrop",
      on: {
        click: (e) => {
          if (e.target === e.currentTarget) close();
        },
        keydown: (e) => {
          if (e.key === "Escape") close();
        },
      },
    },
    h(
      "div",
      { class: "sheet whatsnew", role: "dialog", attrs: { "aria-modal": "true", "aria-label": w.title } },
      h("div", { class: "sheet-head" }, h("h2", { text: w.title })),
      h("div", { class: "sheet-body" }, w.sections.map((s) => sectionBlock(s, w.sections.length > 1 || !forUpdate))),
      h(
        "div",
        { class: "sheet-foot" },
        forUpdate && u && !u.busy ? button("Update now", () => void app.installUpdate(), "btn primary") : null,
        button("Close", close, forUpdate ? "btn" : "btn primary"),
      ),
    ),
  );
}

/** The quiet banner: "Roundtable 0.1.3 is ready. What's new". */
export function renderUpdateBanner(app: App): HTMLElement | null {
  const u = app.state.update;
  if (!u || (u.dismissed && !u.busy)) return null;
  if (u.busy === "installing") {
    return h("div", { class: "banner", role: "status" }, h("span", { text: `Updating to Roundtable ${u.version}. Roundtable restarts by itself.` }));
  }
  return h(
    "div",
    { class: "banner", role: "status" },
    h("span", { text: `Roundtable ${u.version} is ready.` }),
    h("button", { class: "btn small ghost", type: "button", text: "What's new", on: { click: () => app.showUpdateNotes() } }),
    h("button", { class: "btn small primary", type: "button", text: "Update now", on: { click: () => void app.installUpdate() } }),
    h("button", { class: "btn small", type: "button", text: "Later", on: { click: () => app.laterUpdate() } }),
  );
}

function timeOf(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/** One line under Check now. */
export function updateStatusLine(app: App): string {
  const { update: u, updateStatus: st, settings } = app.state;
  if (st.checking) return "Checking for updates...";
  if (u?.busy === "downloading") return `Downloading Roundtable ${u.version}...`;
  if (u) {
    if (u.ready && settings.auto_update) return `Roundtable ${u.version} is downloaded. It installs when you quit Roundtable, or now with Update now.`;
    return `Roundtable ${u.version} is ready.`;
  }
  if (st.error) return `Could not check for updates: ${st.error}`;
  if (st.checkedAt) return `You have the latest version. Checked at ${timeOf(st.checkedAt)}.`;
  return "";
}

/** Settings, Updates. */
export function updatesSection(app: App): HTMLElement[] {
  const s = app.state;
  const test = s.settings.update_channel === "test";
  const status = updateStatusLine(app);
  return [
    h("p", { text: `Version ${s.version || "unknown"}${test ? ", Test builds" : ""}` }),
    toggle(
      "Install updates automatically",
      s.settings.auto_update,
      (v) => void app.setAutoUpdate(v),
      "Downloads in the background and installs the next time you quit Roundtable.",
    ),
    toggle("Test builds", test, (v) => void app.setUpdateChannel(v ? "test" : "stable"), "Get early versions before everyone else. They can have rough edges."),
    h(
      "div",
      { class: "actions left" },
      button("Check now", () => void app.checkUpdate(true), "btn", s.updateStatus.checking),
      s.update && !s.update.busy ? button("Update now", () => void app.installUpdate(), "btn primary") : null,
      button("What's new", () => app.showChangelog(), "btn ghost"),
    ),
    h("p", { class: "small muted update-status", text: status, attrs: { "aria-live": "polite" } }),
  ];
}
