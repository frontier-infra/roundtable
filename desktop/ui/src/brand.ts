// Roundtable's own marks: the table with six seats, the AI logos and the interface glyphs.
// The rules behind them are in desktop/BRAND.md.

import { h } from "./dom";
import { providerName } from "./providers";

import claudeLogo from "./assets/logos/claude.svg";
import codexLogo from "./assets/logos/codex.svg";
import geminiLogo from "./assets/logos/gemini.svg";
import openaiLogo from "./assets/logos/openai.svg";
import grokLogo from "./assets/logos/grok.svg";
import glmLogo from "./assets/logos/z-ai.svg";
import minimaxLogo from "./assets/logos/minimax-icon.svg";
import qwenLogo from "./assets/logos/qwen-icon.svg";
import kimiLogo from "./assets/logos/moonshot-ai-icon.svg";
import opencodeLogo from "./assets/logos/opencode-icon.svg";
import cursorLogo from "./assets/logos/cursor.svg";

const NS = "http://www.w3.org/2000/svg";

const LOGOS: Record<string, string> = {
  claude: claudeLogo,
  codex: codexLogo,
  gemini: geminiLogo,
  openai: openaiLogo,
  grok: grokLogo,
  glm: glmLogo,
  minimax: minimaxLogo,
  qwen: qwenLogo,
  kimi: kimiLogo,
  opencode: opencodeLogo,
  cursor: cursorLogo,
};

/** The six heads the welcome screen seats, in table order. */
export const WELCOME_SEATS = ["claude", "openai", "gemini", "grok", "glm", "minimax"];

function svg(viewBox: string, cls: string): SVGSVGElement {
  const el = document.createElementNS(NS, "svg");
  el.setAttribute("viewBox", viewBox);
  el.setAttribute("class", cls);
  el.setAttribute("aria-hidden", "true");
  el.setAttribute("focusable", "false");
  return el;
}

function add(parent: Element, tag: string, attrs: Record<string, string>): SVGElement {
  const e = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
  parent.appendChild(e);
  return e;
}

/** Seat positions round a table at (c, c), six seats, the chair at the top. */
function seats(c: number, r: number): [number, number, number][] {
  return Array.from({ length: 6 }, (_, i) => {
    const a = -Math.PI / 2 + (i * Math.PI) / 3;
    return [c + r * Math.cos(a), c + r * Math.sin(a), a];
  });
}

/** One seat glyph: a dot with an open arc behind it, facing the table. */
function seatGlyph(parent: Element, x: number, y: number, a: number, cls: string) {
  const r = 4.6;
  const from = a - (2 * Math.PI) / 3;
  const to = a + (2 * Math.PI) / 3;
  const p = (t: number) => `${(x + r * Math.cos(t)).toFixed(2)} ${(y + r * Math.sin(t)).toFixed(2)}`;
  add(parent, "path", { d: `M ${p(from)} A ${r} ${r} 0 1 1 ${p(to)}`, class: `seat-arc ${cls}` });
  add(parent, "circle", { cx: x.toFixed(2), cy: y.toFixed(2), r: "2.7", class: `seat-dot ${cls}` });
}

/** The Roundtable mark: a round table, six seats, the chair's seat in the brand colour. */
export function mark(cls = "logo"): HTMLElement {
  const el = svg("0 0 64 64", cls);
  add(el, "circle", { cx: "32", cy: "32", r: "13", class: "mark-table" });
  seats(32, 23.5).forEach(([x, y, a], i) => seatGlyph(el, x, y, a, i === 0 ? "chair" : ""));
  return el as unknown as HTMLElement;
}

/** The mark and the name, as one lockup. */
export function lockup(cls = "lockup"): HTMLElement {
  return h("div", { class: cls }, mark(), h("span", { class: "wordmark", text: "Roundtable" }));
}

/** An AI's own logo on a light tile, so dark and colour marks both read. */
export function headLogo(head: string, size: "sm" | "md" | "lg" = "md"): HTMLElement {
  const src = LOGOS[head];
  const tile = h("span", { class: `head-logo ${size}`, attrs: { "data-logo": head } });
  if (src) tile.appendChild(h("img", { attrs: { src, alt: "", width: "20", height: "20", draggable: "false" } }));
  else tile.appendChild(h("span", { class: "head-initial", text: providerName(head).slice(0, 1) }));
  return tile;
}

/**
 * The table, drawn large: up to six seats, each filled by an AI's logo or left empty.
 * A seventh AI and on are counted under the table.
 */
export function tableScene(heads: string[], label: string): HTMLElement {
  const seated = heads.slice(0, 6);
  const extra = heads.length - seated.length;
  const ring = svg("0 0 200 200", "table-ring");
  add(ring, "circle", { cx: "100", cy: "100", r: "54", class: "table-top" });
  const pos = seats(100, 82);
  const wrap = h("figure", { class: "table-scene", attrs: { role: "img", "aria-label": label } }, ring);
  pos.forEach(([x, y, a], i) => {
    const head = seated[i];
    if (head) {
      const t = headLogo(head, "lg");
      t.classList.add("seated");
      t.style.left = `${x / 2}%`;
      t.style.top = `${y / 2}%`;
      wrap.appendChild(t);
    } else {
      const g = document.createElementNS(NS, "g");
      g.setAttribute("transform", `translate(${x} ${y}) scale(1.6) translate(${-x} ${-y})`);
      seatGlyph(g, x, y, a, i === 0 ? "chair" : "");
      ring.appendChild(g);
    }
  });
  if (extra > 0) wrap.appendChild(h("span", { class: "table-extra", text: `+${extra}` }));
  return wrap;
}

// ---- Interface glyphs (Lucide, ISC licence) ----

const GLYPHS: Record<string, string> = {
  settings:
    "M9.671 4.136a2.34 2.34 0 0 1 4.659 0a2.34 2.34 0 0 0 3.319 1.915a2.34 2.34 0 0 1 2.33 4.033a2.34 2.34 0 0 0 0 3.831a2.34 2.34 0 0 1-2.33 4.033a2.34 2.34 0 0 0-3.319 1.915a2.34 2.34 0 0 1-4.659 0a2.34 2.34 0 0 0-3.32-1.915a2.34 2.34 0 0 1-2.33-4.033a2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915M15 12a3 3 0 1 1-6 0a3 3 0 0 1 6 0",
  plus: "M5 12h14m-7-7v14",
  search: "m21 21l-4.34-4.34M19 11a8 8 0 1 1-16 0a8 8 0 0 1 16 0",
  back: "m12 19l-7-7l7-7m7 7H5",
  check: "M20 6L9 17l-5-5",
  alert: "M12 8v4m0 4h.01M22 12a10 10 0 1 1-20 0a10 10 0 0 1 20 0",
  file: "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2zM14 2v5a1 1 0 0 0 1 1h5M10 9H8m8 4H8m8 4H8",
  chevron: "m9 18l6-6l-6-6",
  x: "M18 6L6 18M6 6l12 12",
};

export function glyph(name: keyof typeof GLYPHS | string, cls = "glyph"): HTMLElement {
  const el = svg("0 0 24 24", cls);
  add(el, "path", { d: GLYPHS[name] ?? "", fill: "none", stroke: "currentColor", "stroke-width": "2", "stroke-linecap": "round", "stroke-linejoin": "round" });
  return el as unknown as HTMLElement;
}
