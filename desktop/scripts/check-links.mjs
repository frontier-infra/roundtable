#!/usr/bin/env node
// Checks every link a person can click in the app: the "Where do I get a key?" pages,
// the help pages and the Claude download page. A dead link fails the build.
// A sign-in wall (401 or 403) counts as alive: the page exists, it wants a login.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
// providers.ts, the credits (Titanium, Jason's page) and the Get Jebadiah pages.
const read = (f) => readFileSync(join(here, "..", "ui", "src", f), "utf8");
const src = ["providers.ts", "credits.ts", "jebadiah.ts"].map(read).join("\n");
// Each Jebadiah build's Hugging Face page, from the BUILDS table.
const jebPages = [...read("jebadiah.ts").matchAll(/(?:gguf|mlx): "(jebadiah-[^"]+)"/g)].map((m) => `https://huggingface.co/frontier-infra/${m[1]}`);
const urls = [...new Set([...[...src.matchAll(/"(https:\/\/[^"]+)"/g)].map((m) => m[1]), ...jebPages])];
// The issues link takes query text; check the issues page itself.
const targets = urls.map((u) => u.replace(/\/issues\/new$/, "/issues"));

let failed = 0;
for (const url of targets) {
  let status = 0;
  try {
    const res = await fetch(url, {
      redirect: "follow",
      headers: { "user-agent": "Mozilla/5.0 (Roundtable link check)" },
      signal: AbortSignal.timeout(20000),
    });
    status = res.status;
  } catch (e) {
    status = -1;
  }
  const ok = (status >= 200 && status < 400) || status === 401 || status === 403;
  if (!ok) failed++;
  console.log(`${ok ? "ok  " : "DEAD"} ${status} ${url}`);
}
if (failed) {
  console.error(`${failed} dead link(s)`);
  globalThis.process.exit(1);
}
