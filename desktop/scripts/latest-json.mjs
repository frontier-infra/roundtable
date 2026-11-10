#!/usr/bin/env node
// Writes the Tauri updater manifest (latest.json) for one desktop release.
// Usage: node latest-json.mjs <folder with updater bundles and .sig files> <tag> [release that holds the files]
//   tag: desktop-v0.1.2 (stable) or desktop-v0.1.2-rc.1 (test build)
//   release: where the app downloads from; defaults to the tag. Test builds live on desktop-test.
// The notes are that version's section of desktop/CHANGELOG.md.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { sectionFor } from "./changelog.mjs";

const REPO = "https://github.com/frontier-infra/roundtable/releases/download";

export function manifest({ files, sig, tag, release = tag, changelog, now = new Date() }) {
  const version = tag.replace(/^desktop-v/, "");
  const base = `${REPO}/${release}`;
  const has = (f) => files.includes(f);
  const platforms = {};
  // The bundle names carry the version, so test builds can share one release.
  const mac = files.find((f) => f.endsWith(".app.tar.gz") && f.includes(`_${version}_`) && has(`${f}.sig`));
  if (mac) {
    // One universal build serves both Mac chips.
    const entry = { signature: sig(mac), url: `${base}/${encodeURIComponent(mac)}` };
    platforms["darwin-aarch64"] = entry;
    platforms["darwin-x86_64"] = entry;
  }
  const win = files.find((f) => f.endsWith("-setup.exe") && f.includes(`_${version}_`) && has(`${f}.sig`));
  if (win) platforms["windows-x86_64"] = { signature: sig(win), url: `${base}/${encodeURIComponent(win)}` };
  if (!Object.keys(platforms).length) throw new Error(`no signed updater bundles for ${version}`);
  const notes = sectionFor(changelog, version).body;
  return { version, notes, pub_date: now.toISOString(), platforms };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [dir, tag, release] = process.argv.slice(2);
  try {
    const changelog = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", "CHANGELOG.md"), "utf8");
    const files = readdirSync(dir);
    const sig = (f) => readFileSync(join(dir, `${f}.sig`), "utf8").trim();
    console.log(JSON.stringify(manifest({ files, sig, tag, release, changelog }), null, 2));
  } catch (e) {
    console.error(`::error::${e.message}`);
    process.exit(1);
  }
}
