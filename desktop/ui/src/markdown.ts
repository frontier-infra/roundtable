// A small, safe Markdown renderer for AI answers: headings, lists, bold, italic,
// inline code, code blocks, rules and paragraphs. Builds DOM nodes; never uses innerHTML.

import { h } from "./dom";

/** Help pages only: links and pictures. AI answers never get them (no links, no remote images). */
export interface MarkdownOptions {
  /** Builds a link. Without it, `[text](href)` stays plain text. */
  link?: (href: string, text: string) => Node;
  /** A picture on a line of its own: `![alt](src)`. Return null to drop it. */
  image?: (src: string, alt: string) => HTMLElement | null;
  /** How far headings move down: `#` becomes h(1 + shift). Default 2 (answers sit under the page's own headings). */
  headingShift?: number;
}

function inline(text: string, opts: MarkdownOptions = {}): Node[] {
  const out: Node[] = [];
  const link = opts.link ? "\\[[^\\]]+\\]\\([^)\\s]+\\)|" : "";
  const re = new RegExp(`(${link}\\*\\*[^*]+\\*\\*|\`[^\`]+\`|\\*[^*\\s][^*]*\\*|_[^_\\s][^_]*_)`, "g");
  let last = 0;
  for (const m of text.matchAll(re)) {
    const i = m.index ?? 0;
    if (i > last) out.push(document.createTextNode(text.slice(last, i)));
    const t = m[0];
    const lm = opts.link && t.startsWith("[") ? /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(t) : null;
    if (lm) out.push(opts.link!(lm[2], lm[1]));
    else if (t.startsWith("**")) out.push(h("strong", { text: t.slice(2, -2) }));
    else if (t.startsWith("`")) out.push(h("code", { text: t.slice(1, -1) }));
    else out.push(h("em", { text: t.slice(1, -1) }));
    last = i + t.length;
  }
  if (last < text.length) out.push(document.createTextNode(text.slice(last)));
  return out;
}

export function renderMarkdown(src: string, opts: MarkdownOptions = {}): HTMLElement {
  const shift = opts.headingShift ?? 2;
  const root = h("div", { class: "md" });
  const lines = src.replace(/\r\n/g, "\n").split("\n");
  let i = 0;
  let para: string[] = [];
  const flush = () => {
    if (para.length) {
      root.appendChild(h("p", null, inline(para.join(" "), opts)));
      para = [];
    }
  };
  while (i < lines.length) {
    const line = lines[i];
    const t = line.trim();
    if (t.startsWith("```")) {
      flush();
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith("```")) code.push(lines[i++]);
      i++;
      root.appendChild(h("pre", null, h("code", { text: code.join("\n") })));
      continue;
    }
    const hm = /^(#{1,6})\s+(.*)$/.exec(t);
    if (hm) {
      flush();
      const level = Math.min(6, hm[1].length + shift);
      root.appendChild(h(`h${level}` as "h3", null, inline(hm[2], opts)));
      i++;
      continue;
    }
    const im = opts.image ? /^!\[([^\]]*)\]\(([^)\s]+)\)$/.exec(t) : null;
    if (im) {
      flush();
      const pic = opts.image!(im[2], im[1]);
      if (pic) root.appendChild(pic);
      i++;
      continue;
    }
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(t)) {
      flush();
      root.appendChild(h("hr"));
      i++;
      continue;
    }
    if (/^([-*+]|\d+[.)])\s+/.test(t)) {
      flush();
      const ordered = /^\d/.test(t);
      const list = h(ordered ? "ol" : "ul");
      while (i < lines.length && /^\s*([-*+]|\d+[.)])\s+/.test(lines[i])) {
        list.appendChild(h("li", null, inline(lines[i].trim().replace(/^([-*+]|\d+[.)])\s+/, ""), opts)));
        i++;
      }
      root.appendChild(list);
      continue;
    }
    if (t === "") {
      flush();
      i++;
      continue;
    }
    para.push(t);
    i++;
  }
  flush();
  return root;
}
