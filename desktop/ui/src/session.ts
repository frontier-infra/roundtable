// The question coach, verdict shapes, shared material, follow-ups and the Markdown export.
// Pure functions, no DOM, so they are easy to test.

import { decisionMarkdown } from "./decision";
import type { Attachment, Brief, VerdictShape } from "./api";
import { providerName } from "./providers";
import { cleanAnswer, depthTitle, splitVerdict, type RunView, type SeatPick } from "./state";

export const VERDICT_SHAPES: { id: VerdictShape; label: string; hint: string }[] = [
  { id: "decision", label: "Decision", hint: "One answer, with how sure the table is." },
  { id: "pros-cons", label: "Pros and cons", hint: "The case for and against the leading option." },
  { id: "plan", label: "Action plan", hint: "Numbered steps, who does each, and by when." },
  { id: "vote", label: "Vote", hint: "Each seat's pick, and the count." },
];

export function verdictTitle(shape: VerdictShape | string): string {
  return VERDICT_SHAPES.find((v) => v.id === shape)?.label ?? "Verdict";
}

export function verdictHint(shape: VerdictShape | string): string {
  return VERDICT_SHAPES.find((v) => v.id === shape)?.hint ?? "";
}

// ---- The coach's brief ----

export const BRIEF_FIELDS: { key: "goal" | "context" | "constraints" | "good_answer"; label: string; md: string; placeholder: string }[] = [
  { key: "goal", label: "Goal", md: "Goal", placeholder: "What you want to decide or achieve." },
  { key: "context", label: "Context", md: "Context", placeholder: "The facts that matter." },
  { key: "constraints", label: "Constraints", md: "Constraints", placeholder: "Limits on money, time, people or risk." },
  { key: "good_answer", label: "What a good answer looks like", md: "A good answer", placeholder: "Its form, and what it must cover." },
];

export function emptyBrief(goal = ""): Brief {
  return { goal, context: "", constraints: "", good_answer: "" };
}

/** The brief as the table reads it. Same shape as the engine's `Brief::to_markdown`. */
export function briefMarkdown(b: Brief): string {
  return BRIEF_FIELDS.map((f) => [f.md, (b[f.key] ?? "").split(/\s+/).filter(Boolean).join(" ")] as const)
    .filter(([, v]) => v)
    .map(([k, v]) => `- **${k}:** ${v}`)
    .join("\n");
}

// ---- Shared material ----

export const SHARE_LIMIT = 12000;

/** The material every seat reads. Same text as the engine's `attach::block`. */
export function attachmentsBlock(list: Attachment[]): string {
  if (!list.length) return "";
  let out = "Every seat reads the same material.\n";
  for (const a of list) {
    const note = a.shared === "summary" ? " (summary)" : a.shared === "start" ? ` (first ${SHARE_LIMIT} characters)` : "";
    out += `\n### ${a.name}${note}\n${a.text.trim()}\n`;
  }
  return out.trimEnd();
}

/** The text the coach reads: the question, then the material every seat will read. Same words as coach::with_material. */
export function coachText(question: string, list: Attachment[]): string {
  const m = attachmentsBlock(list).trim();
  return m ? `${question.trim()}\n\nATTACHED MATERIAL (every seat already reads this; do not ask for it):\n${m}` : question;
}

/** The line under an attachment's name. */
export function attachmentNote(a: Attachment): string {
  const size = a.chars >= 1000 ? `${Math.round(a.chars / 100) / 10}k characters` : `${a.chars} characters`;
  const kind = a.kind === "pdf" ? "PDF" : a.kind === "link" ? "Link" : "Text";
  if (a.shared === "summary") return `${kind}, ${size}. Summarized once; every seat reads the summary.`;
  if (a.shared === "start") return `${kind}, ${size}. Too long to summarize now, so every seat reads the start.`;
  return `${kind}, ${size}. Every seat reads all of it.`;
}

/** A short label for a link or file. */
export function attachmentLabel(a: Attachment): string {
  if (a.kind !== "link") return a.name;
  return a.name.replace(/^https?:\/\//, "").replace(/\/$/, "");
}

export function looksLikeLink(s: string): boolean {
  return /^https?:\/\/[^\s/]+\.[^\s]+$/i.test(s.trim()) || /^https?:\/\/(localhost|127\.0\.0\.1)(:\d+)?(\/\S*)?$/i.test(s.trim());
}

// ---- The verdict ----

/** The verdict body without the minority lines, and the minority view and who held it. */
export function splitFinalVerdict(text: string): { body: string; minority?: string; heldBy?: string } {
  let minority: string | undefined;
  let heldBy: string | undefined;
  const keep: string[] = [];
  for (const line of text.split("\n")) {
    const t = line.replace(/\*\*/g, "").replace(/^\s*[-*]\s*/, "").trim();
    const m = /^minority view:\s*(.*)$/i.exec(t);
    const hb = /^held by:\s*(.*)$/i.exec(t);
    if (m) minority = m[1].trim();
    else if (hb) heldBy = hb[1].trim();
    else keep.push(line);
  }
  const none = (v?: string) => (!v || /^["']?(none|nobody)["'.]?$/i.test(v) ? undefined : v);
  return { body: keep.join("\n").trim(), minority: none(minority), heldBy: none(heldBy) };
}

const KEY_LINE = /^\s*\**\s*(QUESTIONS FIRST|DECISION|CONFIDENCE|WOULD CHANGE IT|NEXT STEPS|OPTION|RECOMMENDATION|GOAL|FIRST STEP TODAY|RESULT)\s*:\s*\**\s*(.*)$/i;

/** The verdict's "KEY: value" lines as their own bold-led paragraphs, so they never run together. */
export function formatVerdictBody(body: string): string {
  const out: string[] = [];
  for (const line of body.split("\n")) {
    const m = KEY_LINE.exec(line);
    if (m) {
      const key = m[1].toLowerCase();
      out.push("", `**${key[0].toUpperCase()}${key.slice(1)}:** ${m[2].replace(/\*\*$/, "").trim()}`.trimEnd(), "");
    } else out.push(line);
  }
  return out.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

/** "high", "medium" or "low" from the decision text, when there is one. */
export function confidenceOf(v: { confidence?: string; text: string }): string | undefined {
  if (v.confidence) return v.confidence;
  const m = /confidence:\s*\**\s*(high|medium|low)/i.exec(v.text);
  return m ? m[1].toLowerCase() : undefined;
}

// ---- Seats ----

/** "CFO on Claude", or "Claude" on a Generic seat. */
export function seatName(run: RunView, key: string): string {
  const c = run.cards[key];
  const head = c?.head ?? run.seats[key]?.head ?? key;
  const p = c?.persona ?? run.seats[key]?.persona;
  return p ? `${p} on ${providerName(head)}` : providerName(head);
}

/** The seat a key names: `<persona>@<head>` (with `#2` on repeats) or the head id. */
export function seatPickOf(key: string): SeatPick {
  const k = key.replace(/#\d+$/, "");
  const at = k.lastIndexOf("@");
  return at < 0 ? { persona: "", head: k } : { persona: k.slice(0, at), head: k.slice(at + 1) };
}

/** The seats of a run, to ask the same table again. */
export function runSeats(run: RunView): SeatPick[] {
  return run.heads.map(seatPickOf);
}

// ---- Follow-ups ----

const PRIOR_ANSWER = 1500;

function clip(s: string, n: number): string {
  const t = s.trim();
  return t.length > n ? `${t.slice(0, n - 1)}…` : t;
}

/** What every seat reads about the earlier turns of this session. */
export function priorText(turns: RunView[]): string {
  return turns
    .map((r, i) => {
      const lines = [`### Turn ${i + 1}: ${r.question}`];
      if (r.target && r.target !== "table") lines.push(`_Asked of one seat: ${seatName(r, r.target)}._`);
      if (r.brief) lines.push("", "Brief:", r.brief);
      lines.push("", "Final answers:");
      for (const k of r.heads) {
        const c = r.cards[k];
        if (c?.text) lines.push(`- **${seatName(r, k)}:** ${clip(cleanAnswer(c.text).replace(/\s*\n\s*/g, " "), PRIOR_ANSWER)}`);
      }
      if (r.verdict) lines.push("", `Verdict (${verdictTitle(r.verdict.shape).toLowerCase()}):`, r.verdict.text.trim());
      else {
        const last = r.verdicts[r.verdicts.length - 1];
        if (last) lines.push("", "Chair:", splitVerdict(last.text).body);
      }
      return lines.join("\n");
    })
    .join("\n\n");
}

// ---- Markdown export ----

function roundTitle(run: RunView, round: number): string {
  if (round > 1) return `Round ${round}: they read each other and revised`;
  return run.openFirstRound ? "Round 1: each seat read the seats before it" : "Round 1: each seat answered on its own";
}

function tableRow(cells: string[]): string {
  return `| ${cells.map((c) => c.replace(/\|/g, "\\|").replace(/\n/g, " ")).join(" | ")} |`;
}

/** One turn as Markdown: the brief, the seats (persona and AI), each round, and the verdict. */
export function runMarkdown(run: RunView, opts: { title?: string; date?: Date | null } = {}): string {
  const out: string[] = [`# ${opts.title ?? `Roundtable: ${run.question}`}`, ""];
  if (opts.date !== null) out.push(`_${(opts.date ?? new Date()).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}_`, "");
  if (opts.title) out.push(`**Question:** ${run.question}`, "");
  if (run.target && run.target !== "table") out.push(`_Asked of one seat: ${seatName(run, run.target)}._`, "");
  out.push(`**Mode:** ${depthTitle(run.depth)}${run.rounds > 1 ? `, up to ${run.rounds} rounds` : ", one round"}`, "");
  if (run.brief) out.push("## Brief", "", run.brief.trim(), "");
  out.push("## Seats", "", tableRow(["Seat", "Persona", "AI"]), "|---|---|---|");
  run.heads.forEach((k, i) => {
    const c = run.cards[k];
    const info = run.seats[k];
    const head = c?.head ?? info?.head ?? k;
    out.push(tableRow([String(i + 1), c?.persona ?? info?.persona ?? "Generic", providerName(head)]));
  });
  out.push("");
  const labels = run.heads.map((k) => run.cards[k]?.label ?? run.seats[k]?.label).filter((x): x is string => !!x);
  if (labels.length) out.push(`_${[...new Set(labels)].join(" ")}_`, "");
  if (run.attachments?.length) out.push("## Material every seat read", "", ...run.attachments.map((a) => `- ${a}`), "");
  for (const st of run.steps) {
    out.push(`## ${roundTitle(run, st.round)}`, "");
    for (const k of run.heads) {
      const a = st.answers[k];
      if (!a) continue;
      out.push(`### ${seatName(run, k)}`, "", a.text ? cleanAnswer(a.text).trim() : "_No answer this round._", "");
    }
    if (st.verdict) {
      out.push(`### Chair after round ${st.round}: ${st.verdict.consensus ? "they agree" : "they still differ"}`, "", splitVerdict(st.verdict.text).body, "");
    }
  }
  if (run.verdict) {
    const v = splitFinalVerdict(run.verdict.text);
    out.push(`## Verdict: ${verdictTitle(run.verdict.shape)}`, "", formatVerdictBody(v.body), "");
    const conf = confidenceOf(run.verdict);
    if (run.verdict.shape === "decision" && conf) out.push(`**Confidence:** ${conf}`, "");
    const minority = run.verdict.minority ?? v.minority;
    const heldBy = run.verdict.held_by ?? v.heldBy;
    out.push(`**Minority view:** ${minority ?? "None. Every seat backed the verdict."}`, "");
    if (minority) out.push(`**Held by:** ${heldBy ?? "Not named"}`, "");
  }
  if (run.decision) out.push(decisionMarkdown(run.decision), "");
  return out.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n";
}

/** The whole session: every turn, oldest first. */
export function sessionMarkdown(turns: RunView[], date?: Date): string {
  if (turns.length === 1) return runMarkdown(turns[0], { date });
  const head = [`# Roundtable session: ${turns[0].question}`, "", `_${turns.length} turns. ${(date ?? new Date()).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}_`, ""];
  const body = turns.map((r, i) => runMarkdown(r, { title: i === 0 ? `Turn 1` : `Turn ${i + 1}: follow-up`, date: null }).replace(/^(#{1,5}) /gm, "#$1 "));
  return [...head, body.join("\n---\n\n")].join("\n").trimEnd() + "\n";
}

/** A file name for a turn or session. */
export function mdFileName(question: string, session = false): string {
  const s = question
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48)
    .replace(/-+$/, "");
  return `roundtable-${session ? "session-" : ""}${s || "answer"}.md`;
}
