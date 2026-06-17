<div align="center">

# 🪑 Roundtable

### A council of frontier models — for the decisions that matter.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)
[![A Frontier Infra project](https://img.shields.io/badge/Frontier_Infra-project-ff2e88)](https://frontierinfra.org)
[![Website](https://img.shields.io/badge/web-roundtable.sh-0b0a0f)](https://roundtable.sh)
[![CLI · MCP](https://img.shields.io/badge/CLI_·_MCP-any_harness-444)](#mcp--wire-into-your-harness)

**One question → many minds → one decision.**

Fan a question out to **Grok, Codex/OpenAI, GLM, MiniMax, Claude, and Gemini** in parallel,
optionally **deliberate to consensus** with a chaired vote, and get back one decided answer —
from the command line or any MCP-capable coding harness.

</div>

```bash
curl -fsSL https://roundtable.sh/install.sh | bash
```

---

## Why Roundtable?

Every model has a blind spot it can't see. Right before an irreversible call — an
architecture, a migration, a rewrite — is exactly when one model's confident answer
deserves a second (and third, and fourth) opinion.

Roundtable convenes a **panel of frontier models**. Each answers independently; then, for
high-stakes calls, a **multi-round deliberation** lets every head read the whole table and
revise until a **Claude chair** declares `CONSENSUS` or names the remaining disagreement.
Disagreement is signal, not noise.

- **🗳️ Advisory** *(default)* — every head answers once, blind to the others. Raw, independent reads.
- **⚖️ Deliberation** (`--rounds N`) — heads see each other and revise; a neutral chair rules `CONSENSUS` / `CONTINUE` and stops early on agreement.
- **🔑 API keys are enough** — no local CLIs required. Missing heads are skipped; the council runs on whatever subset you've configured.
- **📦 One binary, everywhere** — a CLI, an MCP server, and a one-command installer that wires itself into your coding harness.

## Example

```text
$ roundtable "Reply with exactly: OK three words" --heads claude,gemini

# 🪑 Round Table — 2 heads — advisory
## 🔵 Claude (claude-opus-4-8, Anthropic)      → OK three words
## ✨ Gemini (gemini-3.1-pro-preview, Google)  → OK three words
```

Add `--rounds 3` and the heads deliberate; a chair prints `VERDICT: CONSENSUS` (or
`CONTINUE`) with the points they agreed on and any dissent named.

## Install

```bash
curl -fsSL https://roundtable.sh/install.sh | bash
```

Or pick your package manager — all four deliver the same engine:

```bash
pip install roundtable
uv tool install roundtable
brew install frontier-infra/tap/roundtable
```

The installer places the engine under `${XDG_DATA_HOME:-~/.local/share}/roundtable`,
symlinks `roundtable` into `~/.local/bin`, and prints a `PATH` hint if needed.

## Quickstart

```bash
# 1. Configure keys (masked prompts; shows which heads are ✓/✗). Stored chmod-600
#    in ~/.config/roundtable/config.env. Any subset works — missing heads are skipped.
roundtable auth

# 2. Ask the council
roundtable "Postgres or SQLite for a single-tenant internal tool?"

# 3. Deliberate to consensus on a high-stakes call
roundtable ask -q "Should we rewrite the billing service in Go?" --rounds 3
```

`roundtable doctor` shows which heads are configured and pings each for reachability.

### Flags

`-q/--question` · `-c/--context <file>` (shared background for every head) ·
`--heads grok,codex,openai,glm,minimax,claude,gemini` · `--rounds N` (deliberation) ·
`--research` (web on, multi-step) · `--timeout <secs>` · `--out <file>`.

Exit 0 if at least one head answered.

## The heads

| Head | Provider | Key / auth |
|------|----------|------------|
| 🤖 **Grok** | xAI | `XAI_API_KEY` (or local `grok` CLI / Hermes OAuth) |
| 🧠 **Codex** | OpenAI | local `codex` CLI (read-only) |
| 🧠 **OpenAI** | OpenAI | `OPENAI_API_KEY` (direct API — no local CLI needed) |
| 🟣 **GLM** | Z.AI | `ZAI_API_KEY` |
| 🟠 **MiniMax** | MiniMax | `MINIMAX_API_KEY` |
| 🔵 **Claude** | Anthropic | `ANTHROPIC_API_KEY` (also the deliberation **chair**) |
| ✨ **Gemini** | Google | `GEMINI_API_KEY` / `GOOGLE_API_KEY` |

Heads with no key/CLI are skipped inline — the council runs on whatever subset you've
configured, so **API keys alone are enough** (no local CLIs required).

## MCP + wire into your harness

Roundtable ships an MCP server as a subcommand of the one binary:

```bash
roundtable mcp serve     # stdio JSON-RPC; one tool: roundtable(question, heads?, rounds?, research?)
roundtable mcp config    # prints the server JSON block to paste manually
```

Auto-wire detected harnesses (idempotent — backs up first, never clobbers):

```bash
roundtable install                  # detect + wire everything present
roundtable install --harness cursor,codex
roundtable install --all            # non-interactive
```

Wires **Claude Code** (skill + MCP), **Cursor** (`~/.cursor/mcp.json`),
**Codex CLI** (`~/.codex/config.toml`), and prints a generic JSON block for anything else:

```json
{ "command": "roundtable", "args": ["mcp", "serve"] }
```

## Use in Claude Code

Roundtable ships as a **Claude Code plugin** — a `roundtable` skill that drives the CLI.
Two ways to add it:

**A — Plugin marketplace** (inside Claude Code):

```text
/plugin marketplace add frontier-infra/roundtable
/plugin install roundtable@roundtable
```

The skill calls the **globally-installed `roundtable` CLI**, so install that first:
`curl -fsSL https://roundtable.sh/install.sh | bash`.

**B — `roundtable install`** — auto-wires the skill **and** the MCP server into Claude
Code (plus Cursor / Codex) in one step:

```bash
roundtable install
```

## Commands

```
roundtable "question"          shorthand for `ask`
roundtable ask                 run the council (all engine flags)
roundtable auth [provider]     configure API keys (masked, chmod-600)
roundtable models              per-head model picker
roundtable heads               enable/disable heads
roundtable doctor              which heads are configured + reachability ping
roundtable config              show / edit config.env
roundtable mcp serve|config    MCP stdio server / print config
roundtable install             auto-wire detected harnesses
roundtable version | update    show version / upgrade in place
```

## For agents

Building on top of Roundtable? See [`AGENTS.md`](./AGENTS.md) — a concise guide for an
AI agent to install, configure, run, and wire Roundtable into its own harness.

## Links

- **Website & docs** — https://roundtable.sh
- **Source** — https://github.com/frontier-infra/roundtable
- **Frontier Infra** — https://frontierinfra.org

## License

[MIT](./LICENSE) © Frontier Infra
