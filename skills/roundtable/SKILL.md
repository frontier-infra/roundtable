---
name: roundtable
description: Convene a "round table" of multiple frontier AI models — Grok, Codex/OpenAI, GLM, MiniMax, Claude, and Gemini — to get independent perspectives on a question, optionally deliberating across rounds until a Claude chair declares consensus, then synthesize a single recommendation. Use when the user says "round table", "roundtable", "council", "put your heads together", "ask grok", "ask codex", "get a second opinion", "what do the other models think", "consult the council", "deliberate", or wants cross-model consensus before a decision. Runs via the `roundtable` MCP tool or the `roundtable` CLI (heads use API keys; Grok/Codex can also use their local CLIs). Can run a chaired deliberation-to-consensus.
---

# 🪑 Round Table

Fan a curated question out to up to **six heads** in **parallel**, collect their
independent answers, and — for high-stakes calls — run multiple rounds where each head
sees the others and revises until a Claude **chair** declares consensus. You (this
session) are the moderator: you curate the question, decide what context each head gets,
and synthesize one decided answer.

Two ways to run it:

- **The `roundtable` MCP tool.** The Claude Code plugin starts it for you. It uses the API
  keys the user entered in the plugin settings (`/plugin`, then Roundtable, then Configure).
  Prefer it when it is available.
- **The `roundtable` command** in the Bash tool. The plugin puts it on the PATH. Outside the
  plugin, install it with `uv tool install roundtable-sh`. It reads keys from
  `~/.config/roundtable/config.env`, which `roundtable auth` writes. The user runs
  `roundtable auth` in their own terminal, because it prompts for keys.

The heads:

| key | head | auth |
|-----|------|------|
| `grok` | 🤖 Grok (xAI) | `XAI_API_KEY` (or local `grok` CLI / Hermes OAuth) |
| `codex` | 🧠 Codex / GPT (OpenAI) | local `codex` CLI |
| `openai` | 🧠 OpenAI (direct API) | `OPENAI_API_KEY` |
| `glm` | 🟣 GLM (Z.AI) | `ZAI_API_KEY` |
| `minimax` | 🟠 MiniMax | `MINIMAX_API_KEY` |
| `claude` | 🔵 Claude (Anthropic) | `ANTHROPIC_API_KEY` |
| `gemini` | ✨ Gemini (Google) | `GEMINI_API_KEY` / `GOOGLE_API_KEY` |

Claude also serves as the neutral **chair** that judges consensus in deliberation mode.
Heads with a missing key/CLI are skipped inline — the table still returns whatever the
rest produced. Run `roundtable doctor` to see which heads are configured.

## When to use it

- Before committing to an architecture, approach, or irreversible decision — a cross-model gut-check.
- When the user asks to "ask the other models", "convene the council", or "deliberate".
- When independent confirmation (or a flagged disagreement) raises confidence more than Claude alone.
- For a quick single-head sanity check (`--heads grok`) when you just want one outside opinion fast.

Don't use it for trivial questions or where the next action is already dictated. It costs
wall-clock and tokens/credits on the other accounts.

## How to run it

```bash
# Advisory (default): all configured heads answer once, blind to each other
roundtable "Your question here"
roundtable ask -q "Your question here"

# Subset of heads
roundtable ask --heads grok,codex,claude -q "Your question"

# Deliberation: 2 to 4 rounds, each head sees the whole table and revises; stops at consensus
roundtable ask -q "Your question" --rounds 3

# Seat roles: a preset, a pack, or persona names (see Personas below)
roundtable ask -q "Your question" --personas security-review

# Shared background context for every head (write the file first)
roundtable ask -q "Your question" -c /tmp/context.md

# Research mode: web ON + multi-step — for questions needing current/external facts
roundtable ask -q "Your question" --research

# Also save the combined block to a file
roundtable ask -q "Your question" --out /tmp/rt.md
```

Flags: `-q/--question`, `-c/--context <file>`, `--heads grok,codex,openai,glm,minimax,claude,gemini`,
`--rounds N` (1 to 4), `--personas <preset, pack or names>`, `--research`, `--timeout <secs>`, `--out <file>`.

**Two axes that compose:** *rounds* (advisory `--rounds 1`, default → independent blind
answers; deliberation `--rounds 2` to `4` → each head reads the whole table and revises, chair
prints `VERDICT: CONSENSUS|CONTINUE` and stops early on consensus) × *knowledge* (fast,
default → web off; `--research` → web on, multi-step).

## Personas: seat roles

A seat can play a role. A persona has a name, a one-line expertise and a short brief (how
the seat reasons, what it weighs, what it doubts). It sits on an AI seat, so one AI can
hold several personas. Without personas every seat is Generic, as before.

```bash
roundtable personas                                       # list packs, presets and personas
roundtable ask -q "Should we raise prices 20%?" --personas pricing          # a preset
roundtable ask -q "..." --personas business-roles                          # a whole pack
roundtable ask -q "..." --personas cfo,ceo@claude,"Head of Sales"          # names, @ pins the AI
roundtable ask -q "..." --personas frontier-four --rounds 3                # lab views, 3 rounds
```

Built-in packs (plain JSON in the repo's `personas/` folder):

- **Business roles:** CEO, CTO, CFO, COO, CMO, CISO, General Counsel, Head of Product,
  Head of Sales, Customer advocate, Skeptical investor, Board chair. Presets `exec-team`,
  `board-review`.
- **Expert panels:** presets `security-review`, `go-to-market`, `hiring-decision`, `pricing`,
  `architecture-review` (3 to 5 roles each).
- **Frontier labs:** "The NVIDIA view", "The OpenAI view", "The xAI view", "The Anthropic
  view", "The Google DeepMind view", "The Microsoft view", "The AMD view", "The Meta view".
  Presets `frontier-four`, `frontier-all`. Each seat is a simulated perspective drawn from
  public positions. It names no person and never speaks for the lab. Every lab seat
  carries this label in the transcript, and you keep it when you quote that seat:
  "Simulated perspective drawn from public positions. Not affiliated with or endorsed by
  <lab>." `ROUNDTABLE_HIDE_LAB_PACK=1` hides the pack.
- **Generic:** `--personas generic`, or no flag.

Your own packs go in `~/.config/roundtable/personas/*.json` (same shape as the built-ins).
The MCP tool takes the same value in its `personas` argument. In the results table, name
each row by its seat: "CFO (Claude)".

## The Claude workflow (you, the moderator)

1. **Curate the question.** The heads see only what you pass — none of this conversation. Put essential background in a `-c` context file rather than a giant `-q`.
2. **Pick the mode.** Default to advisory + fast. Use `--rounds 2` to `4` for high-stakes convergence; add `--research` only when the answer needs the web. Add `--personas` when the decision needs named expertise (a pricing call, a security review, a hire).
3. **Read all heads + form your own view.** You are a seat at the table — don't just relay.
4. **Synthesize, don't average** — consensus (state as high-confidence), disagreement (name the tension, say which side is more credible and why), recommendation (one decided answer).
5. **Attribute briefly** — one line on what each head contributed.

## Required output (MANDATORY — every round table)

Always return, **in this order, above the prose synthesis**:

**(1) Question recap.** Echo what was asked — main question in one line, plus any sub-questions as a list exactly as posed.

**(2) Results table.** One row per head, **always include every head** (even failures — use `—` and `_(timed out)_` / `_(no answer)_` / `_(unavailable)_` so a missing voice is visible, never silently dropped):

| Model | Stance | Position / key input | In consensus? |
|-------|--------|----------------------|---------------|
| 🤖 Grok | For / Against / Split / — | one-line summary of its actual position | ✅ / ❌ / — |
| 🧠 Codex / OpenAI | … | … | … |
| 🟣 GLM | … | … | … |
| 🟠 MiniMax | … | … | … |
| 🔵 Claude | … | … | … |
| ✨ Gemini | … | … | … |

Stance = For/Against/Split relative to the decision (name which option if multiple).
Keep each cell to one line; the prose above carries the nuance.

**(3) Judge's decision (MANDATORY).** Below the table, surface both layers, clearly labeled:
- **Chair verdict** — the in-script chair's ruling (`CONSENSUS` / `SPLIT` / `NO-AGREEMENT`) **and what it was based on**: the AGREED points plus any named DISSENT. Don't just say "consensus."
- **Moderator's decision (mine)** — your final operator-facing call, flagged as your own and stated separately from the chair. Say whether you **concur** or **override**, and **why**. Never silently adopt or discard the consensus.

Both are required even when they agree ("Moderator concurs with the chair, because…").

## Reliability notes

- **Heads fail independently.** A missing CLI/key, a timeout, or an empty answer is shown inline; the table still returns whatever the others produced. **Exit 0 if ≥1 head answered.**
- If a head errors with an auth message, that key/CLI needs attention — run `roundtable doctor`, then `roundtable auth <provider>` (or `grok login` / `codex login`).
- Model/head overrides live in `~/.config/roundtable/config.env` (`roundtable config` to edit, `roundtable models` / `roundtable heads` to pick).
