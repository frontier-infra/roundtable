#!/usr/bin/env node
// The app's one version number lives in desktop/Cargo.toml, [workspace.package].
// The app, the engine and the UI all read it from there (the UI asks the core).
//
//   node version.mjs              print the version
//   node version.mjs next         print the next release version (the patch digit climbs)
//   node version.mjs bump         write the next release version
//   node version.mjs tag <tag>    check a release tag, print {"version","base","channel"} as JSON
//   node version.mjs stamp <v>    write an exact version, such as 0.1.2-rc.1 (the release workflow)
//   node version.mjs newest <n>   read versions on stdin, print the newest n (test channel cleanup)
//
// Jason's rule: the patch digit climbs to 99 (0.1.8, 0.1.9 ... 0.1.99), then 0.2.0.
// Test builds carry a pre-release suffix, rc.N with a dot, so 0.1.2-rc.10 sorts after 0.1.2-rc.9.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DESKTOP = join(dirname(fileURLToPath(import.meta.url)), "..");
const CARGO = join(DESKTOP, "Cargo.toml");
const LOCK = join(DESKTOP, "Cargo.lock");
const CRATES = ["roundtable-engine", "roundtable-desktop"];

const RELEASE = /^(\d+)\.(\d+)\.(\d+)$/;
const TEST = /^(\d+)\.(\d+)\.(\d+)-rc\.([1-9]\d*)$/;

export function readVersion(toml) {
  const i = toml.indexOf("[workspace.package]");
  const m = i >= 0 && toml.slice(i).match(/^version\s*=\s*"([^"]+)"/m);
  if (!m) throw new Error("no version in desktop/Cargo.toml [workspace.package]");
  return m[1];
}

/** Changes only the version line of [workspace.package]. Keeps CRLF line ends (a Windows checkout). */
export function writeVersion(toml, version) {
  const i = toml.indexOf("[workspace.package]");
  if (i < 0) throw new Error("no [workspace.package] in desktop/Cargo.toml");
  const rest = toml.slice(i).replace(/^version([ \t]*=[ \t]*)"[^"\r\n]+"/m, `version$1"${version}"`);
  return toml.slice(0, i) + rest;
}

/** The workspace crates' entries in Cargo.lock carry the version too. */
export function writeLockVersion(lock, version) {
  let out = lock;
  for (const name of CRATES) {
    out = out.replace(new RegExp(`(name = "${name}"\\r?\\nversion = ")[^"]+(")`), `$1${version}$2`);
  }
  return out;
}

/** The release after this one: the patch digit climbs to 99, then the minor digit moves. */
export function nextVersion(version) {
  const m = version.match(RELEASE) ?? version.match(TEST);
  if (!m) throw new Error(`not a version: ${version}`);
  const [maj, min, pat] = m.slice(1, 4).map(Number);
  // A test build of 0.1.2 is released as 0.1.2.
  if (TEST.test(version)) return `${maj}.${min}.${pat}`;
  if (pat >= 99) return `${maj}.${min + 1}.0`;
  return `${maj}.${min}.${pat + 1}`;
}

/** desktop-v0.1.2 is a stable release; desktop-v0.1.2-rc.1 is a test build of 0.1.2. */
export function parseTag(tag, appVersion) {
  const version = String(tag).replace(/^refs\/tags\//, "").replace(/^desktop-v/, "");
  if (!String(tag).includes("desktop-v")) throw new Error(`not a desktop tag: ${tag}`);
  let channel;
  if (RELEASE.test(version)) channel = "stable";
  else if (TEST.test(version)) channel = "test";
  else throw new Error(`tag ${tag} is not desktop-vX.Y.Z or desktop-vX.Y.Z-rc.N (rc.N with a dot, so rc.10 sorts after rc.9)`);
  const base = version.replace(/-.*$/, "");
  if (appVersion !== undefined && base !== appVersion) {
    throw new Error(`tag ${tag} is for ${base}, but desktop/Cargo.toml says ${appVersion}. Run node desktop/scripts/version.mjs bump first.`);
  }
  return { version, base, channel };
}

/** Semver order for our versions: 0.1.2-rc.1 < 0.1.2-rc.2 < 0.1.2 < 0.1.3. */
export function compareVersions(a, b) {
  const parse = (v) => {
    const m = v.match(/^(\d+)\.(\d+)\.(\d+)(?:-rc\.(\d+))?$/);
    if (!m) throw new Error(`not a version: ${v}`);
    return [Number(m[1]), Number(m[2]), Number(m[3]), m[4] === undefined ? Infinity : Number(m[4])];
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < 4; i++) if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
  return 0;
}

/** The newest n versions, oldest first. Anything that is not one of our versions is left out. */
export function newest(list, n) {
  const ok = list.filter((v) => RELEASE.test(v) || TEST.test(v));
  return [...new Set(ok)].sort(compareVersions).slice(-n);
}

function stamp(version) {
  if (!RELEASE.test(version) && !TEST.test(version)) throw new Error(`not a version: ${version}`);
  writeFileSync(CARGO, writeVersion(readFileSync(CARGO, "utf8"), version));
  writeFileSync(LOCK, writeLockVersion(readFileSync(LOCK, "utf8"), version));
  return version;
}

function main(argv) {
  const [cmd, arg] = argv;
  const current = readVersion(readFileSync(CARGO, "utf8"));
  switch (cmd) {
    case undefined:
      return current;
    case "next":
      return nextVersion(current);
    case "bump":
      return stamp(nextVersion(current));
    case "stamp":
      return stamp(arg);
    case "tag":
      return JSON.stringify(parseTag(arg, current));
    case "newest":
      return newest(readFileSync(0, "utf8").split(/\s+/), Number(arg) || 1).join("\n");
    default:
      throw new Error(`unknown command: ${cmd}`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    console.log(main(process.argv.slice(2)));
  } catch (e) {
    console.error(`::error::${e.message}`);
    process.exit(1);
  }
}
