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

## Install

### In Claude

Paste these two lines into Claude Code:

```text
/plugin marketplace add frontier-infra/roundtable
/plugin install roundtable@frontier-infra
```

That is all. The plugin brings the `roundtable` command, the Roundtable skill, and the
`roundtable` MCP tool. Claude Code then asks for your API keys. Every key is optional, and
any one key is enough to start. Run `/roundtable:setup` to see which models are ready.

### Command line

```bash
uv tool install roundtable-sh
```

Other ways, one line each:

```bash
pipx install roundtable-sh
pip install roundtable-sh
uv tool install git+https://github.com/frontier-infra/roundtable   # works now, before the PyPI release
pipx install git+https://github.com/frontier-infra/roundtable      # same, with pipx
```

The package is `roundtable-sh`. The command is `roundtable`. The PyPI package named
`roundtable` is an unrelated project, so do not install it. Then run `roundtable auth` to
add your keys.

### Or run our installer

```bash
curl -fsSL https://roundtable.sh/install.sh | bash
```

What it does:

- Checks that `bash`, `python3`, and `curl` are present.
- Downloads Roundtable from this GitHub repository into `~/.local/share/roundtable`.
- Links the `roundtable` command into `~/.local/bin`, and tells you if that folder is not
  on your `PATH`.
- Offers to run `roundtable auth` and `roundtable install`. Both are optional.

It needs no `sudo` and changes nothing else. Read it first at
https://roundtable.sh/install.sh.

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
- **📦 One package, everywhere:** a Claude Code plugin, a CLI, and an MCP server from the same code.

## Example

```text
$ roundtable "Reply with exactly: OK three words" --heads claude,gemini

# 🪑 Round Table — 2 heads — advisory
## 🔵 Claude (claude-opus-4-8, Anthropic)      → OK three words
## ✨ Gemini (gemini-3.1-pro-preview, Google)  → OK three words
```

Add `--rounds 3` and the heads deliberate; a chair prints `VERDICT: CONSENSUS` (or
`CONTINUE`) with the points they agreed on and any dissent named.

## Quickstart

```bash
# 1. Configure keys (masked prompts; shows which heads are ✓/✗). Stored chmod-600
#    in ~/.config/roundtable/config.env. Any subset works — missing heads are skipped.
roundtable auth

# 2. Ask the council
roundtable "Postgres or SQLite for a single-tenant internal tool?"

# 3. Deliberate to consensus on a high-stakes call
roundtable ask -q "Should we rewrite the billing service in Go?" --rounds 3
roundtable ask -q "Should we raise prices 20%?" --personas pricing    # seat roles (personas/)

# 4. Sharpen the question, get a verdict you can paste, read a file first
roundtable ask -q "Should we raise prices 20%?" --coach --verdict decision --attach ./pricing.pdf
roundtable ask -q "And if a competitor cuts prices?" --follow-up /tmp/rt.md --personas auto
```

`roundtable doctor` shows which heads are configured and pings each for reachability.

### Flags

`-q/--question` · `-c/--context <file>` (shared background for every head) ·
`--heads grok,codex,openai,glm,minimax,claude,gemini` · `--rounds N` (1 to 4, deliberation) · `--personas SPEC` (seat roles, see `personas/README.md`) ·
`--research` (web on, multi-step) · `--timeout <secs>` · `--out <file>` ·
`--coach` (up to three clarifying questions, then a brief; `--coach-answer A` answers in order, `--brief FILE` brings your own) ·
`--verdict decision|pros-cons|plan|vote|none` (on a table of two or more, a decision by default: the pick, its confidence, what would change it and three next steps lead the transcript, the seat answers fold under it, and it names the minority view and who held it) · `--no-clarify` (skip the missing-facts check: with no brief, the chair opens with two or three questions when the pick depends on facts you did not give) ·
`--no-blind` (round 1 open; blind is the default) · `--attach <file, PDF or link>` (read once; long ones summarized once and shared) ·
`--follow-up <transcript>` · `--personas auto` (suggested seats).

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
roundtable mcp serve     # stdio JSON-RPC; one tool: roundtable(question, heads?, rounds?, personas?, research?, coach?, coach_answers?, verdict?, blind?, attach?)
roundtable mcp config    # prints the server JSON block to paste manually
```

Add it to Claude Code with one line (needs the command line install):

```bash
claude mcp add --scope user roundtable -- roundtable mcp serve
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

Install the plugin with the two lines under [In Claude](#in-claude). It adds:

- `/roundtable:ask <question>`: ask the council and get one decision back.
- `/roundtable:setup`: see which models are ready and how to add keys.
- The Roundtable skill, which Claude uses when you say "ask the council" or "get a second opinion".
- The `roundtable` MCP tool and the `roundtable` command.

Already installed the command line version? `roundtable install` wires the skill and the MCP
server into Claude Code, Cursor, and Codex instead.

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

## About

Roundtable is free. Made by Frontier Infra, built by Titanium Computing.

- Need AI in your business? [Titanium Computing](https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=readme)
- [Built by Jason Brashear, CTO](https://titaniumcomputing.com/jason-brashear/?utm_source=roundtable&utm_medium=readme)

## Privacy, trademarks, and decision limits

Roundtable runs locally and does not send prompts, credentials, or outputs to a
Frontier Infra service. It sends each requested prompt and supplied context
directly to the model providers or authenticated local CLIs that the user
configures. Those requests are governed by the selected providers' terms,
privacy policies, retention settings, and charges. Keep secrets and data you
are not authorized to disclose out of council inputs.

Roundtable output is advisory. Agreement among models is not proof that a
recommendation is correct, safe, lawful, or suitable for a production change.
Provider and product names identify compatible services only. Roundtable and
Frontier Infra are independent and are not sponsored or endorsed by those
providers. See [TRADEMARKS.md](./TRADEMARKS.md).

Website policies: [Privacy](https://roundtable.sh/docs/privacy) ·
[Terms](https://roundtable.sh/docs/terms) ·
[Support](https://github.com/frontier-infra/roundtable/issues)

## License

[MIT](./LICENSE) © Frontier Infra
