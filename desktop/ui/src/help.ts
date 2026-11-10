// Help: the built-in help pages, the About sheet and the native menu's actions.
// The pages are docs/app/*.md in the repo, bundled into the app at build time, so help works
// with no network. roundtable.sh can import the same files (docs/app/README.md).

import type { App } from "./app";
import { button, h } from "./dom";
import { glyph, lockup } from "./brand";
import { renderMarkdown } from "./markdown";
import { aboutSection, outLink } from "./credits";
import { ISSUES_URL } from "./providers";
import { helpUrl } from "./state";
import { updateStatusLine } from "./updates";
import { withGlyph } from "./views/main";

/** Help, Online docs. roundtable.sh has no /docs index page yet, so this is the first docs page. */
export const DOCS_URL = "https://roundtable.sh/docs/get-started/";

const RAW = import.meta.glob("../../../docs/app/*.md", { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const IMAGES = import.meta.glob("../../../docs/app/img/*.{png,jpg,webp}", { query: "?url", import: "default", eager: true }) as Record<string, string>;

export interface HelpPage {
  id: string;
  title: string;
  description: string;
  order: number;
  /** The same page on roundtable.sh. */
  online: string;
  body: string;
}

const base = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/** `---` front matter, then the Markdown body. */
export function parsePage(id: string, text: string): HelpPage {
  const src = text.replace(/\r\n/g, "\n");
  const m = /^---\n([\s\S]*?)\n---\n?/.exec(src);
  const meta: Record<string, string> = {};
  for (const line of (m?.[1] ?? "").split("\n")) {
    const kv = /^([a-z_]+):\s*(.*)$/.exec(line.trim());
    if (kv) meta[kv[1]] = kv[2].replace(/^["']|["']$/g, "");
  }
  return {
    id,
    title: meta.title ?? id,
    description: meta.description ?? "",
    order: Number(meta.order ?? 99),
    online: meta.online ?? DOCS_URL,
    body: src.slice(m ? m[0].length : 0).trim(),
  };
}

/** Every page, in order. README.md is for people editing the pages, not a page. */
export const HELP_PAGES: HelpPage[] = Object.entries(RAW)
  .filter(([path]) => base(path) !== "README.md")
  .map(([path, text]) => parsePage(base(path).replace(/\.md$/, ""), text))
  .sort((a, b) => a.order - b.order || a.title.localeCompare(b.title));

export const HELP_HOME = "overview";

export function helpPage(id: string): HelpPage | undefined {
  return HELP_PAGES.find((p) => p.id === id);
}

/** Bundled picture for `img/name.png`, or null. */
export function helpImage(src: string): string | null {
  const name = base(src);
  const hit = Object.entries(IMAGES).find(([path]) => base(path) === name);
  return hit ? hit[1] : null;
}

const plain = (md: string) =>
  md
    .split("\n")
    .map((l) => l.trim().replace(/^(#{1,6}|[-*+]|\d+[.)])\s+/, ""))
    .filter(Boolean)
    .map((l) => (/[.:?!]$/.test(l) ? l : `${l}.`))
    .join(" ")
    .replace(/!\[[^\]]*\]\([^)]*\)\.?/g, " ")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/[#*_`>]/g, "")
    .replace(/\s+/g, " ")
    .trim();

export interface HelpHit {
  page: HelpPage;
  /** A short piece of the page around the first match. */
  snippet: string;
}

/**
 * Search every page. Each word must appear in the page; the title counts most.
 * Returns pages best first, each with a snippet.
 */
export function searchHelp(query: string, pages: HelpPage[] = HELP_PAGES): HelpHit[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const scored: { hit: HelpHit; score: number }[] = [];
  for (const page of pages) {
    const title = page.title.toLowerCase();
    const text = plain(page.body);
    const lower = `${page.description} ${text}`.toLowerCase();
    if (!words.every((w) => title.includes(w) || lower.includes(w))) continue;
    const score = words.reduce((n, w) => n + (title.includes(w) ? 10 : 0) + lower.split(w).length - 1, 0);
    const at = text.toLowerCase().indexOf(words[0]);
    const from = Math.max(0, at - 40);
    const snippet = at < 0 ? page.description : `${from > 0 ? "..." : ""}${text.slice(from, at + 80).trim()}${at + 80 < text.length ? "..." : ""}`;
    scored.push({ hit: { page, snippet }, score });
  }
  return scored.sort((a, b) => b.score - a.score).map((s) => s.hit);
}

function pageBody(app: App, page: HelpPage): HTMLElement {
  return renderMarkdown(page.body, {
    headingShift: 0,
    link: (href, text) => {
      const id = /^([a-z0-9-]+)\.md$/.exec(href)?.[1];
      if (id && helpPage(id)) {
        return h("a", { href: "#", text, on: { click: (e) => { e.preventDefault(); app.openHelp(id); } } });
      }
      if (/^https:\/\//.test(href)) return outLink(app, href, text);
      return document.createTextNode(text);
    },
    image: (src, alt) => {
      const url = helpImage(src);
      if (!url) return null;
      return h("figure", { class: "help-shot" }, h("img", { attrs: { src: url, alt, loading: "lazy" } }));
    },
  });
}

/** The help screen: the page list and search on the left, the page on the right. */
export function renderHelp(app: App): HTMLElement {
  const s = app.state;
  const page = helpPage(s.help.page) ?? helpPage(HELP_HOME) ?? HELP_PAGES[0];
  const hits = s.help.query.trim() ? searchHelp(s.help.query) : null;

  const search = h("input", {
    class: "search",
    type: "search",
    value: s.help.query,
    placeholder: "Search help",
    attrs: { "aria-label": "Search help", spellcheck: "false" },
    on: {
      input: (e) => {
        app.state.help = { ...app.state.help, query: (e.target as HTMLInputElement).value };
        app.render();
        const box = document.querySelector<HTMLInputElement>(".help-nav .search");
        box?.focus();
        box?.setSelectionRange(box.value.length, box.value.length);
      },
      keydown: (e) => {
        if (e.key === "Escape" && app.state.help.query) {
          e.stopPropagation();
          app.set({ help: { ...app.state.help, query: "" } });
        } else if (e.key === "Enter" && hits?.length) app.openHelp(hits[0].page.id);
      },
    },
  });

  const list = hits
    ? hits.length
      ? h(
          "ul",
          { class: "help-list results" },
          hits.map((x) =>
            h(
              "li",
              null,
              h(
                "button",
                { type: "button", class: x.page.id === page.id ? "active" : "", on: { click: () => app.openHelp(x.page.id) } },
                h("span", { class: "help-item-title", text: x.page.title }),
                h("small", { text: x.snippet }),
              ),
            ),
          ),
        )
      : h("p", { class: "small muted pad", text: "No page matches. Try another word, or open the online docs." })
    : h(
        "ul",
        { class: "help-list" },
        HELP_PAGES.map((p) =>
          h(
            "li",
            null,
            h("button", {
              type: "button",
              class: p.id === page.id ? "active" : "",
              text: p.title,
              attrs: p.id === page.id ? { "aria-current": "page" } : {},
              on: { click: () => app.openHelp(p.id) },
            }),
          ),
        ),
      );

  const nav = h(
    "nav",
    { class: "help-nav", attrs: { "aria-label": "Help pages" } },
    h("label", { class: "search-wrap" }, glyph("search", "search-glyph"), search),
    list,
    h(
      "div",
      { class: "help-more" },
      button("What's new", () => app.showChangelog(), "btn small ghost"),
      button("Online docs", () => void app.api.openUrl(DOCS_URL), "btn small ghost"),
      button("Report a problem", () => void app.api.openUrl(reportUrl(app)), "btn small ghost"),
    ),
  );

  const article = h(
    "article",
    { class: "help-page", attrs: { "aria-labelledby": "help-title", "data-page": page.id } },
    h("h1", { id: "help-title", text: page.title }),
    page.description ? h("p", { class: "lead", text: page.description }) : null,
    pageBody(app, page),
    h("p", { class: "help-online small" }, outLink(app, page.online, "Read this page on roundtable.sh")),
  );

  return h(
    "div",
    { class: "settings-page help-view" },
    h("div", { class: "topbar" }, withGlyph(button("Back", () => app.closeHelp(), "btn ghost"), "back"), h("h1", { text: "Help" }), lockup()),
    h("div", { class: "help-body" }, nav, article),
  );
}

export function reportUrl(app: App): string {
  return helpUrl(ISSUES_URL, undefined, app.state.os, app.state.version);
}

/** About Roundtable, from the menu: version, who made it, the links. */
export function renderAbout(app: App): HTMLElement {
  const close = () => app.set({ about: false });
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
      { class: "sheet about", role: "dialog", attrs: { "aria-modal": "true", "aria-label": "About Roundtable" } },
      h("div", { class: "sheet-head" }, lockup()),
      h("div", { class: "sheet-body" }, h("p", { class: "muted", text: "Ask several AIs at once. Get one clear answer." }), aboutSection(app)),
      h("div", { class: "sheet-foot" }, button("Close", close, "btn primary")),
    ),
  );
}

/** A click in the native menu bar (core/src/menu.rs sends its id). */
export async function onMenu(app: App, id: string): Promise<void> {
  if (id.startsWith("help:")) {
    app.openHelp(id.slice(5));
    return;
  }
  switch (id) {
    case "about":
      app.set({ about: true, whatsNew: null });
      return;
    case "check-updates":
      await app.checkUpdate(true);
      if (app.state.update) app.showToast(updateStatusLine(app));
      return;
    case "settings":
      if (app.state.view === "onboarding") app.showToast("Finish setting up first. Settings opens after the last step.");
      else app.set({ view: "settings", about: false });
      return;
    case "show-history":
      if (app.state.view === "onboarding") app.showToast("Finish setting up first.");
      else app.set({ view: "main", about: false });
      return;
    case "whats-new":
      app.showChangelog();
      return;
    case "online-docs":
      await app.api.openUrl(DOCS_URL);
      return;
    case "report":
      await app.api.openUrl(reportUrl(app));
      return;
  }
}
