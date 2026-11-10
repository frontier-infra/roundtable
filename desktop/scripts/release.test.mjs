// node --test desktop/scripts: the version rule, release tags, the changelog and the manifest.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { compareVersions, newest, nextVersion, parseTag, readVersion, writeLockVersion, writeVersion } from "./version.mjs";
import { parseChangelog, sectionFor } from "./changelog.mjs";
import { manifest } from "./latest-json.mjs";

const here = (p) => new URL(p, import.meta.url);

test("the patch digit climbs to 99, then the minor digit moves", () => {
  assert.equal(nextVersion("0.1.1"), "0.1.2");
  assert.equal(nextVersion("0.1.9"), "0.1.10");
  assert.equal(nextVersion("0.1.98"), "0.1.99");
  assert.equal(nextVersion("0.1.99"), "0.2.0");
  assert.equal(nextVersion("0.1.2-rc.4"), "0.1.2", "a test build is released under its own number");
});

test("test builds sort before their release, rc.10 after rc.9", () => {
  const order = ["0.1.1", "0.1.2-rc.1", "0.1.2-rc.2", "0.1.2-rc.9", "0.1.2-rc.10", "0.1.2", "0.1.3", "0.1.10", "0.2.0"];
  const shuffled = [...order].reverse();
  assert.deepEqual(shuffled.sort(compareVersions), order);
});

test("the test channel keeps the newest two builds", () => {
  assert.deepEqual(newest(["0.1.2-rc.9", "0.1.2-rc.10", "0.1.2-rc.2", "junk", "0.1.2-rc.10", ""], 2), ["0.1.2-rc.9", "0.1.2-rc.10"]);
});

test("release tags", () => {
  assert.deepEqual(parseTag("desktop-v0.1.2", "0.1.2"), { version: "0.1.2", base: "0.1.2", channel: "stable" });
  assert.deepEqual(parseTag("refs/tags/desktop-v0.1.2-rc.3", "0.1.2"), { version: "0.1.2-rc.3", base: "0.1.2", channel: "test" });
  assert.throws(() => parseTag("desktop-v0.1.2-rc3", "0.1.2"), /rc\.N with a dot/);
  assert.throws(() => parseTag("desktop-v0.1.1-rc.1", "0.1.2"), /Cargo\.toml says 0\.1\.2/);
  assert.throws(() => parseTag("v0.1.2", "0.1.2"), /not a desktop tag/);
});

test("one version source: Cargo.toml, stamped into Cargo.lock", () => {
  const toml = readFileSync(here("../Cargo.toml"), "utf8");
  const v = readVersion(toml);
  assert.match(v, /^\d+\.\d+\.\d+$/, "the repo holds a release version; the workflow stamps rc.N");
  const stamped = writeVersion(toml, "9.8.7-rc.1");
  assert.equal(readVersion(stamped), "9.8.7-rc.1");
  assert.equal(stamped.replace("9.8.7-rc.1", v), toml, "only the version line changes");
  const lock = readFileSync(here("../Cargo.lock"), "utf8");
  for (const name of ["roundtable-engine", "roundtable-desktop"]) {
    assert.ok(lock.includes(`name = "${name}"\nversion = "${v}"`), `${name} in Cargo.lock is ${v}`);
    assert.ok(writeLockVersion(lock, "9.8.7-rc.1").includes(`name = "${name}"\nversion = "9.8.7-rc.1"`));
  }
  const pkg = JSON.parse(readFileSync(here("../ui/package.json"), "utf8"));
  assert.equal(pkg.version, undefined, "the UI has no version of its own; it asks the core");
  const conf = JSON.parse(readFileSync(here("../core/tauri.conf.json"), "utf8"));
  assert.equal(conf.version, undefined, "Tauri reads the version from Cargo");
});

test("the changelog has a section for the app's version, in plain words", () => {
  const text = readFileSync(here("../CHANGELOG.md"), "utf8");
  const v = readVersion(readFileSync(here("../Cargo.toml"), "utf8"));
  const s = sectionFor(text, `${v}-rc.1`);
  assert.equal(s.version, v);
  const sections = parseChangelog(text);
  assert.equal(sections[0].version, v, "newest first");
  for (const x of sections) {
    for (const h of x.body.match(/^### .*/gm) ?? []) assert.match(h, /^### (Added|Improved|Fixed)$/);
    assert.ok(!/\u2014/.test(x.body), "no em dashes");
  }
  assert.throws(() => sectionFor(text, "0.0.1"), /no "## 0\.0\.1" section/);
});

test("the manifest points at the right release and carries the notes", () => {
  const changelog = "# x\n\n## 0.1.2 - 2026-11-10\n\n### Fixed\n\n- A thing.\n\n## 0.1.1\n\n- Old.\n";
  const files = [
    "Roundtable_0.1.2-rc.2_universal.app.tar.gz", "Roundtable_0.1.2-rc.2_universal.app.tar.gz.sig",
    "Roundtable_0.1.2-rc.1_universal.app.tar.gz", "Roundtable_0.1.2-rc.1_universal.app.tar.gz.sig",
    "Roundtable_0.1.2-rc.2_x64-setup.exe", "Roundtable_0.1.2-rc.2_x64-setup.exe.sig",
  ];
  const m = manifest({ files, sig: (f) => `sig of ${f}`, tag: "desktop-v0.1.2-rc.2", release: "desktop-test", changelog, now: new Date(0) });
  assert.equal(m.version, "0.1.2-rc.2");
  assert.equal(m.notes, "### Fixed\n\n- A thing.");
  assert.equal(m.platforms["darwin-aarch64"].url, "https://github.com/frontier-infra/roundtable/releases/download/desktop-test/Roundtable_0.1.2-rc.2_universal.app.tar.gz");
  assert.equal(m.platforms["darwin-x86_64"].signature, "sig of Roundtable_0.1.2-rc.2_universal.app.tar.gz");
  assert.equal(m.platforms["windows-x86_64"].url, "https://github.com/frontier-infra/roundtable/releases/download/desktop-test/Roundtable_0.1.2-rc.2_x64-setup.exe");
  assert.throws(() => manifest({ files: files.filter((f) => !f.endsWith(".sig")), sig: () => "", tag: "desktop-v0.1.2", changelog }), /no signed updater bundles/);
});
