#!/usr/bin/env node
// Reads desktop/CHANGELOG.md, the one changelog for the app, the GitHub release,
// the update manifest and roundtable.sh/changelog.
//
//   node changelog.mjs section <version>   print that version's section (a test build reads its release's)
//   node changelog.mjs check <version>     fail unless the section exists and has an entry
//
// Format: "## X.Y.Z - YYYY-MM-DD", then "### Added", "### Improved", "### Fixed", each a list.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const FILE = join(dirname(fileURLToPath(import.meta.url)), "..", "CHANGELOG.md");
const HEAD = /^## (\d+\.\d+\.\d+)(?:\s+-\s+(.+?))?\s*$/;

/** Sections, newest first: { version, date, body }. */
export function parseChangelog(text) {
  const out = [];
  let cur = null;
  for (const line of text.replace(/\r\n/g, "\n").split("\n")) {
    const m = line.match(HEAD);
    if (m) {
      cur = { version: m[1], date: m[2] ?? "", lines: [] };
      out.push(cur);
    } else if (cur) cur.lines.push(line);
  }
  return out.map(({ version, date, lines }) => ({ version, date, body: lines.join("\n").trim() }));
}

/** The section for a version. 0.1.2-rc.1 reads 0.1.2's. */
export function sectionFor(text, version) {
  const base = String(version).replace(/^desktop-v/, "").replace(/-.*$/, "");
  const s = parseChangelog(text).find((x) => x.version === base);
  if (!s) throw new Error(`desktop/CHANGELOG.md has no "## ${base}" section. Write it before the release.`);
  if (!/^\s*[-*] \S/m.test(s.body)) throw new Error(`desktop/CHANGELOG.md "## ${base}" has no entries.`);
  return s;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [cmd, version] = process.argv.slice(2);
  try {
    const s = sectionFor(readFileSync(FILE, "utf8"), version);
    if (cmd === "section") console.log(s.body);
    else if (cmd === "check") console.log(`ok: ${s.version}`);
    else throw new Error(`unknown command: ${cmd}`);
  } catch (e) {
    console.error(`::error::${e.message}`);
    process.exit(1);
  }
}
