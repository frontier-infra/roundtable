// Updates and What's new: the check at launch, the quiet banner, the channels, automatic
// download, Update now, and What's new after an update and from Settings.

import { beforeEach, describe, expect, it } from "vitest";
import { App } from "../src/app";
import { mockApi, type MockOptions } from "../src/mock";
import { CHANGELOG, compareVersions, parseChangelog, sectionsBetween, whatsNewAfterUpdate } from "../src/updates";

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = '<div id="app"></div>';
  root = document.getElementById("app")!;
});
const tick = async (n = 10) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};
async function start(opts: MockOptions = {}, at: { view?: "main" | "settings" } = { view: "main" }, settings: Record<string, unknown> = {}) {
  const api = mockApi({ delay: 0, onboarded: true, ...opts });
  if (Object.keys(settings).length) await api.settingsSet({ ...(await api.settingsGet()), ...settings });
  const app = new App(api, root);
  await app.start(at);
  await tick();
  return { app, api };
}
const text = () => root.textContent ?? "";
const buttonNamed = (name: string) => [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;
const switchNamed = (name: string) =>
  [...root.querySelectorAll("label.switch-row")].find((l) => l.querySelector(".switch-text > span")?.textContent === name)?.querySelector("input") as HTMLInputElement | undefined;
const NOTES = "### Fixed\n\n- The verdict card no longer jumps.";

describe("version order", () => {
  it("test builds come before their release, and rc.10 after rc.9", () => {
    const order = ["0.1.1", "0.1.2-rc.1", "0.1.2-rc.2", "0.1.2-rc.10", "0.1.2", "0.1.3", "0.1.99", "0.2.0"];
    expect([...order].reverse().sort(compareVersions)).toEqual(order);
  });
});

describe("the changelog", () => {
  it("ships in the app, newest first, in Added, Improved and Fixed", () => {
    const sections = parseChangelog(CHANGELOG);
    expect(sections.length).toBeGreaterThan(0);
    for (let i = 1; i < sections.length; i++) expect(compareVersions(sections[i - 1].version, sections[i].version)).toBe(1);
    for (const s of sections) for (const head of s.body.match(/^### .*/gm) ?? []) expect(head).toMatch(/^### (Added|Improved|Fixed)$/);
    expect(CHANGELOG).not.toMatch(/\u2014/);
  });

  it("What's new after an update covers every version since the last one seen", () => {
    const text = "## 0.1.4 - d\n\n- four\n\n## 0.1.3 - c\n\n- three\n\n## 0.1.2 - b\n\n- two\n";
    const s = parseChangelog(text);
    expect(sectionsBetween(s, "0.1.2", "0.1.4").map((x) => x.version)).toEqual(["0.1.4", "0.1.3"]);
    expect(sectionsBetween(s, "0.1.2-rc.1", "0.1.2-rc.2").map((x) => x.version)).toEqual(["0.1.2"]);
    expect(whatsNewAfterUpdate("0.1.3", "0.1.4", text)?.title).toBe("Roundtable is now 0.1.4");
    expect(whatsNewAfterUpdate("", "0.1.4", text)).toBeNull();
    expect(whatsNewAfterUpdate("0.1.4", "0.1.4", text)).toBeNull();
    expect(whatsNewAfterUpdate("0.1.5", "0.1.4", text)).toBeNull();
  });
});

describe("the update check", () => {
  it("looks at launch even with automatic updates off, and shows the quiet banner", async () => {
    const { api } = await start({ update: { version: "0.1.3", notes: NOTES } }, { view: "main" }, { auto_update: false });
    expect(api.calls).toContain("updateCheck:stable");
    expect(text()).toContain("Roundtable 0.1.3 is ready.");
    expect(buttonNamed("Update now")).toBeTruthy();
    expect(buttonNamed("Later")).toBeTruthy();
    expect(api.calls.some((c) => c.startsWith("updateDownload"))).toBe(false);
  });

  it("with automatic updates on, downloads in the background to install on quit", async () => {
    const { app, api } = await start({ update: { version: "0.1.3", notes: NOTES } });
    expect(api.calls).toContain("updateDownload:true");
    expect(app.state.update?.ready).toBe(true);
  });

  it("What's new in the banner shows the new version's notes", async () => {
    await start({ update: { version: "0.1.3", notes: NOTES } });
    buttonNamed("What's new")!.click();
    await tick();
    const sheet = root.querySelector(".whatsnew")!;
    expect(sheet.textContent).toContain("What's new in Roundtable 0.1.3");
    expect(sheet.textContent).toContain("The verdict card no longer jumps.");
    expect(sheet.querySelector("li")).toBeTruthy();
  });

  it("Update now installs; Later hides the banner until the next manual check", async () => {
    const { app, api } = await start({ update: { version: "0.1.3", notes: NOTES } });
    buttonNamed("Later")!.click();
    await tick();
    expect(text()).not.toContain("is ready.");
    expect(text()).toContain("It installs the next time you quit Roundtable.");
    await app.checkUpdate(false);
    expect(text()).not.toContain("Roundtable 0.1.3 is ready.");
    await app.checkUpdate(true);
    expect(text()).toContain("Roundtable 0.1.3 is ready.");
    buttonNamed("Update now")!.click();
    await tick();
    expect(api.calls).toContain("updateInstall");
    expect(text()).toContain("Updating to Roundtable 0.1.3");
  });

  it("a failed check says so in plain words, with no raw error", async () => {
    const { app } = await start({}, { view: "settings" });
    app.api.updateCheck = async () => {
      throw "Could not fetch a valid release JSON from the remote";
    };
    await app.checkUpdate(false);
    expect(text()).toContain("Could not check for updates. Roundtable tries again in 6 hours");
    expect(text()).not.toContain("release JSON");
  });

  it("no banner when this is the newest version", async () => {
    await start();
    expect(root.querySelector(".banner")).toBeNull();
  });

  it("the Test builds switch checks the test channel", async () => {
    const { app, api } = await start({ update: { version: "0.1.2-rc.2", notes: NOTES, channel: "test" } }, { view: "settings" });
    expect(text()).not.toContain("is ready.");
    switchNamed("Test builds")!.click();
    await tick();
    expect(app.state.settings.update_channel).toBe("test");
    expect(api.calls).toContain("updateCheck:test");
    expect(text()).toContain("Roundtable 0.1.2-rc.2 is ready.");
    switchNamed("Test builds")!.click();
    await tick();
    expect(app.state.settings.update_channel).toBe("stable");
    expect(app.state.update).toBeNull();
  });

  it("turning automatic updates off stops the install on quit", async () => {
    const { api } = await start({ update: { version: "0.1.3", notes: NOTES } }, { view: "settings" });
    switchNamed("Install updates automatically")!.click();
    await tick();
    expect(api.calls).toContain("updateOnQuit:false");
  });

  it("Settings shows the version, Check now and What's new", async () => {
    const { api } = await start({ version: "0.1.2-rc.1" }, { view: "settings" });
    expect(text()).toContain("Version 0.1.2-rc.1");
    expect(root.querySelector(".about-version")?.textContent).toBe("Roundtable 0.1.2-rc.1");
    buttonNamed("Check now")!.click();
    await tick();
    expect(api.calls.filter((c) => c === "updateCheck:stable").length).toBe(2);
    expect(text()).toContain("You have the latest version.");
    buttonNamed("What's new")!.click();
    await tick();
    const sheet = root.querySelector(".whatsnew")!;
    expect(sheet.textContent).toContain(`Roundtable ${parseChangelog(CHANGELOG)[0].version}`);
  });
});

describe("What's new after an update", () => {
  it("opens once on the first launch of a newer version", async () => {
    const { app, api } = await start({ version: "0.1.2" }, { view: "main" }, { last_seen_version: "0.1.1" });
    expect(root.querySelector(".whatsnew")?.textContent).toContain("Roundtable is now 0.1.2");
    expect(app.state.settings.last_seen_version).toBe("0.1.2");
    expect((await api.settingsGet()).last_seen_version).toBe("0.1.2");
  });

  it("a first install or the same version opens nothing", async () => {
    await start({ version: "0.1.2" });
    expect(root.querySelector(".whatsnew")).toBeNull();
    document.body.innerHTML = '<div id="app"></div>';
    root = document.getElementById("app")!;
    await start({ version: "0.1.2" }, { view: "main" }, { last_seen_version: "0.1.2" });
    expect(root.querySelector(".whatsnew")).toBeNull();
  });
});
