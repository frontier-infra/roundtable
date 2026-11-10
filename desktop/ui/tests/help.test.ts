// Help: the native menu's actions, the built-in help pages (docs/app), search and About.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi } from "../src/mock";
import { DOCS_URL, HELP_PAGES, helpImage, parsePage, searchHelp } from "../src/help";
import { renderMarkdown } from "../src/markdown";

import MENU_RS from "../../core/src/menu.rs?raw";

// Every file in docs/app, found apart from src/help.ts.
const DOC_FILES = Object.keys(import.meta.glob("../../../docs/app/*.md")).map((p) => p.slice(p.lastIndexOf("/") + 1));

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
});
const tick = async (n = 10) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};
async function start(view: "main" | "settings" | null = "main", onboarded = true) {
  const api = mockApi({ delay: 0, onboarded });
  const app = new App(api, root);
  await app.start(view ? { view } : {});
  await tick();
  return { app, api };
}
const text = () => root.textContent ?? "";
const buttonNamed = (name: string) => [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;

describe("the help pages", () => {
  it("bundles every page in docs/app, in order, each with a title, a description and an online twin", () => {
    const files = DOC_FILES.filter((f) => f !== "README.md");
    expect(HELP_PAGES.map((p) => `${p.id}.md`).sort()).toEqual(files.sort());
    expect(HELP_PAGES[0].id).toBe("overview");
    for (const p of HELP_PAGES) {
      expect(p.title, p.id).not.toBe(p.id);
      expect(p.description, p.id).not.toBe("");
      expect(p.online, p.id).toMatch(/^https:\/\/roundtable\.sh\/docs\//);
    }
    const orders = HELP_PAGES.map((p) => p.order);
    expect(orders).toEqual([...orders].sort((a, b) => a - b));
  });

  it("every Help menu page in the Rust menu has a page with the same title", () => {
    const menu = [...MENU_RS.matchAll(/\("help:([a-z-]+)", "([^"]+)"\)/g)].map((m) => [m[1], m[2]]);
    expect(menu.length).toBeGreaterThanOrEqual(6);
    for (const [id, label] of menu) expect(HELP_PAGES.find((p) => p.id === id)?.title, id).toBe(label);
  });

  it("every picture a page uses is bundled, every page link goes to a page, and no page has an em dash", () => {
    const ids = new Set(HELP_PAGES.map((p) => p.id));
    for (const p of HELP_PAGES) {
      for (const m of p.body.matchAll(/!\[[^\]]*\]\(([^)]+)\)/g)) expect(helpImage(m[1]), `${p.id}: ${m[1]}`).not.toBeNull();
      for (const m of p.body.matchAll(/(?<!!)\[[^\]]+\]\(([^)]+)\)/g)) {
        if (m[1].endsWith(".md")) expect(ids.has(m[1].replace(/\.md$/, "")), `${p.id}: ${m[1]}`).toBe(true);
      }
      expect(p.body.includes("—"), p.id).toBe(false);
    }
  });

  it("reads front matter", () => {
    const p = parsePage("x", "---\ntitle: Hello\ndescription: A page.\norder: 3\nonline: https://roundtable.sh/docs/x/\n---\n\n## Body\n");
    expect(p).toMatchObject({ id: "x", title: "Hello", description: "A page.", order: 3, online: "https://roundtable.sh/docs/x/", body: "## Body" });
  });
});

describe("search", () => {
  it("finds pages by any word, the title first, with a snippet", () => {
    const hits = searchHelp("keychain");
    expect(hits.map((x) => x.page.id)).toContain("connect-your-ais");
    expect(hits[0].snippet.toLowerCase()).toContain("keychain");
    expect(searchHelp("jebadiah")[0].page.id).toBe("decision-model");
    expect(searchHelp("keyboard shortcuts")[0].page.id).toBe("keyboard-shortcuts");
  });

  it("needs every word, and finds nothing for nonsense", () => {
    expect(searchHelp("zzzqqq")).toEqual([]);
    expect(searchHelp("")).toEqual([]);
    expect(searchHelp("jebadiah zzzqqq")).toEqual([]);
  });
});

describe("the help screen", () => {
  it("Help, Roundtable Help opens the overview inside the app, with no network", async () => {
    const { app, api } = await start();
    api.menu("help:overview");
    await tick();
    expect(app.state.view).toBe("help");
    expect(root.querySelector(".help-page h1")?.textContent).toBe("Roundtable Help");
    expect(api.calls.filter((c) => c.startsWith("open:"))).toEqual([]);
    // The page list shows every page.
    expect([...root.querySelectorAll(".help-list button")].map((b) => b.textContent)).toEqual(HELP_PAGES.map((p) => p.title));
  });

  it("each menu page opens that page, with its pictures and a link to its online twin", async () => {
    const { api } = await start();
    api.menu("help:connect-your-ais");
    await tick();
    expect(root.querySelector(".help-page")?.getAttribute("data-page")).toBe("connect-your-ais");
    expect(root.querySelectorAll(".help-shot img").length).toBeGreaterThanOrEqual(2);
    const online = [...root.querySelectorAll<HTMLAnchorElement>(".help-online a")][0];
    expect(online.textContent).toBe("Read this page on roundtable.sh");
    online.click();
    expect(api.calls).toContain(`open:${HELP_PAGES.find((p) => p.id === "connect-your-ais")!.online}`);
  });

  it("a link between pages opens the other page in the app", async () => {
    const { app, api } = await start();
    api.menu("help:overview");
    await tick();
    const link = [...root.querySelectorAll<HTMLAnchorElement>(".help-page .md a")].find((a) => a.textContent === "Keyboard shortcuts")!;
    link.click();
    expect(app.state.help.page).toBe("keyboard-shortcuts");
    expect(text()).toContain("Cmd+Enter asks the table");
  });

  it("the search box narrows the list, Enter opens the best page, Escape clears it", async () => {
    const { app, api } = await start();
    api.menu("help:overview");
    await tick();
    const box = root.querySelector<HTMLInputElement>(".help-nav .search")!;
    box.value = "persona";
    box.dispatchEvent(new Event("input"));
    expect(root.querySelector(".help-list.results")).not.toBeNull();
    expect(root.querySelector(".help-list.results .help-item-title")?.textContent).toBe("Personas and rounds");
    root.querySelector<HTMLInputElement>(".help-nav .search")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(app.state.help.page).toBe("personas-and-rounds");
    app.set({ help: { ...app.state.help, query: "zzzqqq" } });
    expect(text()).toContain("No page matches.");
    root.querySelector<HTMLInputElement>(".help-nav .search")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(app.state.help.query).toBe("");
  });

  it("Back returns to the screen help was opened from", async () => {
    const { app, api } = await start("settings");
    api.menu("help:getting-started");
    await tick();
    api.menu("help:decision-model");
    await tick();
    buttonNamed("Back")!.click();
    expect(app.state.view).toBe("settings");
  });

  it("Settings, Help pages opens the built-in help, not the browser", async () => {
    const { app, api } = await start("settings");
    buttonNamed("Help pages")!.click();
    expect(app.state.view).toBe("help");
    expect(api.calls.some((c) => c.startsWith("open:"))).toBe(false);
  });

  it("works during first-run setup too", async () => {
    const { app, api } = await start(null, false);
    expect(app.state.view).toBe("onboarding");
    api.menu("help:connect-your-ais");
    await tick();
    expect(app.state.view).toBe("help");
    buttonNamed("Back")!.click();
    expect(app.state.view).toBe("onboarding");
  });
});

describe("the rest of the menu", () => {
  it("What's new opens the changelog sheet", async () => {
    const { app, api } = await start();
    api.menu("whats-new");
    await tick();
    expect(app.state.whatsNew?.title).toBe("What's new");
    expect(root.querySelector(".sheet.whatsnew")).not.toBeNull();
  });

  it("Online docs and Report a problem open the browser; the report carries the version and system", async () => {
    const { api } = await start();
    api.menu("online-docs");
    api.menu("report");
    await tick();
    expect(api.calls).toContain(`open:${DOCS_URL}`);
    const report = api.calls.find((c) => c.startsWith("open:https://github.com/frontier-infra/roundtable/issues/new?"))!;
    expect(decodeURIComponent(report)).toContain("System: macos");
  });

  it("About opens a sheet with the version and the credits; Escape closes it", async () => {
    const { app, api } = await start();
    api.menu("about");
    await tick();
    const sheet = root.querySelector(".sheet.about")!;
    expect(sheet.textContent).toContain(`Roundtable ${app.state.version}`);
    expect(sheet.textContent).toContain("Titanium Computing");
    root.querySelector(".sheet-backdrop")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(app.state.about).toBe(false);
  });

  it("Settings opens Settings, except during first-run setup", async () => {
    const { app, api } = await start();
    api.menu("settings");
    expect(app.state.view).toBe("settings");
    api.menu("show-history");
    expect(app.state.view).toBe("main");
    const fresh = await start(null, false);
    fresh.api.menu("settings");
    expect(fresh.app.state.view).toBe("onboarding");
    expect(fresh.app.state.toast).toContain("Finish setting up first");
  });

  it("Check for updates checks now and says the result", async () => {
    const { api } = await start();
    const before = api.calls.filter((c) => c.startsWith("updateCheck")).length;
    api.menu("check-updates");
    await tick();
    expect(api.calls.filter((c) => c.startsWith("updateCheck")).length).toBe(before + 1);
    expect(text()).toContain("You have the latest version.");
  });
});

describe("Markdown for help pages", () => {
  it("links and pictures only when asked: AI answers never get them", () => {
    const md = "See [the site](https://example.com).\n\n![a screen](img/x.png)";
    const plain = renderMarkdown(md);
    expect(plain.querySelector("a")).toBeNull();
    expect(plain.querySelector("img")).toBeNull();
    const rich = renderMarkdown(md, {
      link: (href, t) => Object.assign(document.createElement("a"), { href, textContent: t }),
      image: (src, alt) => Object.assign(document.createElement("img"), { src, alt }),
    });
    expect(rich.querySelector("a")?.textContent).toBe("the site");
    expect(rich.querySelector("img")?.getAttribute("alt")).toBe("a screen");
  });
});
