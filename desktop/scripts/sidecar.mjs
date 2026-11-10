#!/usr/bin/env node
// Builds the Rust engine and places it where Tauri expects the sidecar:
// core/binaries/roundtable-engine-<target triple>[.exe].
// Target: TAURI_ENV_TARGET_TRIPLE (set by `cargo tauri build --target`), else the host.
// universal-apple-darwin builds both Mac targets and joins them with lipo.
// ROUNDTABLE_NO_LAB_PACK=1 builds the engine without the Frontier labs persona pack.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, chmodSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const env = globalThis.process.env;
const desktop = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetDir = env.CARGO_TARGET_DIR || join(desktop, "target");
const host = /host: (\S+)/.exec(execFileSync("rustc", ["-vV"]).toString())[1];
const triple = env.TAURI_ENV_TARGET_TRIPLE || host;
const debug = env.TAURI_ENV_DEBUG === "true";
const profile = debug ? "debug" : "release";
const exe = triple.includes("windows") ? ".exe" : "";
const outDir = join(desktop, "core", "binaries");
mkdirSync(outDir, { recursive: true });

function build(t) {
  const args = ["build", "-p", "roundtable-engine", "--target", t];
  if (!debug) args.push("--release");
  if (env.ROUNDTABLE_NO_LAB_PACK === "1") args.push("--no-default-features");
  execFileSync("cargo", args, { cwd: desktop, stdio: "inherit", env: { ...env, CARGO_TARGET_DIR: targetDir } });
  return join(targetDir, t, profile, `roundtable-engine${exe}`);
}

const dest = join(outDir, `roundtable-engine-${triple}${exe}`);
if (triple === "universal-apple-darwin") {
  const arches = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
  const parts = arches.map(build);
  execFileSync("lipo", ["-create", "-output", dest, ...parts], { stdio: "inherit" });
  // Tauri compiles the app once per chip and checks the sidecar for each one.
  for (const [i, t] of arches.entries()) {
    const one = join(outDir, `roundtable-engine-${t}`);
    copyFileSync(parts[i], one);
    chmodSync(one, 0o755);
  }
} else {
  copyFileSync(build(triple), dest);
}
if (!exe) chmodSync(dest, 0o755);
console.log(`sidecar: ${dest}`);
