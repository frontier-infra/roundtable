#!/usr/bin/env node
// Writes the Tauri updater manifest (latest.json) for one desktop release.
// Usage: node latest-json.mjs <folder with installers and .sig files> <tag, e.g. desktop-v0.1.1>
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const [dir, tag] = globalThis.process.argv.slice(2);
const version = tag.replace(/^desktop-v/, "");
const base = `https://github.com/frontier-infra/roundtable/releases/download/${tag}`;
const files = readdirSync(dir);
const sig = (f) => readFileSync(join(dir, `${f}.sig`), "utf8").trim();
const platforms = {};

const mac = files.find((f) => f.endsWith(".app.tar.gz"));
if (mac) {
  // One universal build serves both Mac chips.
  const entry = { signature: sig(mac), url: `${base}/${encodeURIComponent(mac)}` };
  platforms["darwin-aarch64"] = entry;
  platforms["darwin-x86_64"] = entry;
}
const win = files.find((f) => f.endsWith("-setup.exe"));
if (win) platforms["windows-x86_64"] = { signature: sig(win), url: `${base}/${encodeURIComponent(win)}` };

if (!Object.keys(platforms).length) {
  console.error("no updater artifacts found");
  globalThis.process.exit(1);
}
console.log(JSON.stringify({ version, notes: `Roundtable ${version}`, pub_date: new Date().toISOString(), platforms }, null, 2));
