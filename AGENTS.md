<!-- graft:start -->
## Graft , repo context graph

This repo is indexed in `graft/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here , understanding how something works, finding where code lives,
or scoping a change , get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `graft map` first , a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

- Run `graft ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer , cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete , run `graft grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
- `graft skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
- `graft callers <symbol>` gives precomputed, exact edges , who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
- Or browse: `graft/INDEX.md` lists every node; follow the links.
- Monorepos and folders of multiple repos rank fairly across sub-projects ,
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `graft ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to , never
re-read whole files.

After big code changes, refresh the graph with `graft build` (deterministic,
no API key, $0).
<!-- graft:end -->

# AGENTS.md — Roundtable

A guide for coding agents (and the humans wiring them up). Roundtable is a CLI + MCP
server that convenes a **council of frontier models** on a question and, optionally,
runs a **chaired deliberation to consensus**. It's a [Frontier Infra](https://frontierinfra.org)
project — we eat our own cooking: our own agents call Roundtable before the decisions
that matter.

## What it is

One command, several model "heads", run in parallel:

- **Advisory** (default) — every configured head answers once, blind to the others.
- **Deliberation** (`--rounds N`) — heads read each other and revise; a Claude **chair**
  declares `CONSENSUS` / `CONTINUE` and the run stops early on consensus.
- **Research** (`--research`) — web on, multi-step, for answers that need current facts.

Heads: `grok` · `codex` · `openai` · `glm` · `minimax` · `claude` · `gemini`. A head with
no key/CLI is skipped inline — the council runs on whatever subset is configured (so it
works on **API keys alone**, no local CLIs required).

## Install

```bash
# Canonical one-liner
curl -fsSL https://roundtable.sh/install.sh | bash

# Or pip / uv (thin launcher around the same engine)
pip install roundtable
uv tool install roundtable

# Or Homebrew
brew install frontier-infra/tap/roundtable

# Or from a clone
git clone https://github.com/frontier-infra/roundtable && cd roundtable && ./install.sh --from .
```

The installer drops the engine under `${XDG_DATA_HOME:-~/.local/share}/roundtable` and
symlinks `roundtable` into `~/.local/bin`. If that's not on your `PATH`, the installer
prints the exact `export PATH=...` line to add.

## Configure keys

```bash
roundtable auth                 # interactive: masked paste per provider, shows ✓/✗
roundtable auth anthropic       # configure just one provider
roundtable doctor               # which heads are configured + a reachability ping
```

Keys live in `~/.config/roundtable/config.env` (dotenv, `chmod 600`, sourced — no parser).
You can also write it directly:

```bash
mkdir -p ~/.config/roundtable
cat > ~/.config/roundtable/config.env <<'EOF'
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
XAI_API_KEY=xai-...
ZAI_API_KEY=...
MINIMAX_API_KEY=...
GEMINI_API_KEY=...
EOF
chmod 600 ~/.config/roundtable/config.env
```

Per-head model/enable overrides: `roundtable models` and `roundtable heads` (or edit
`config.env`; `roundtable config` shows/edits it).

## Run

```bash
roundtable "Should we use Postgres or SQLite for this service?"     # shorthand
roundtable ask -q "..." --heads grok,claude,gemini                  # subset of heads
roundtable ask -q "..." --rounds 3                                  # deliberate to consensus
roundtable ask -q "..." -c ./context.md                            # shared background for every head
roundtable ask -q "..." --research                                 # web on
roundtable ask -q "..." --out /tmp/rt.md                           # also save the transcript
```

Flags: `-q/--question`, `-c/--context <file>`, `--heads ...`, `--rounds N`, `--research`,
`--timeout <secs>`, `--out <file>`, `--verdict <file>`, `--sign`. Exit 0 if ≥1 head answered.

**Signed verdicts.** `--verdict FILE` emits a machine-consumable record of the run
(question + sha256, per-head answer hashes and status, chair verdict + bullets) so a
ratifier or gate can consume the council's ruling instead of parsing prose. `--sign`
additionally wraps it as an Ed25519-signed
[Agent Attestation Record](https://agentscontrolplane.org); `roundtable sign verdict.json`
signs one after the fact. Both are opt-in and fail-open — an unconfigured signer never
costs you the run or the record. Schema, verdict mapping (CONSENSUS→verified,
CONTINUE→unclear) and key-management options: [docs/signed-verdicts.md](docs/signed-verdicts.md).

**Output discipline:** always return the per-head results table **and** the judge's
decision (chair verdict + your own moderator call). Never summarize without the table.

## Use as MCP / wire into your harness

Roundtable ships an MCP server as a subcommand of the one binary:

```bash
roundtable mcp serve     # stdio JSON-RPC; one tool: roundtable(question, heads?, rounds?, research?)
roundtable mcp config    # prints the server JSON block to paste manually
```

Auto-wire detected harnesses (idempotent — backs up first, never clobbers):

```bash
roundtable install                       # detect + wire everything present
roundtable install --harness cursor,codex
roundtable install --all                 # non-interactive
```

- **Claude Code** — installs this skill into `~/.agents/skills/roundtable/` (and `~/.claude*` profiles) **and** registers the MCP server.
- **Cursor** — `~/.cursor/mcp.json`.
- **Codex CLI** — `~/.codex/config.toml` `[mcp_servers.roundtable]`.
- **generic** — prints the MCP JSON to paste anywhere else.

The MCP server config any harness can use:

```json
{ "command": "roundtable", "args": ["mcp", "serve"] }
```

## Use in Claude Code (plugin)

Roundtable is also importable as a **Claude Code plugin** (the `roundtable` skill). Add it
from the marketplace:

```text
/plugin marketplace add frontier-infra/roundtable
/plugin install roundtable@roundtable
```

The skill drives the globally-installed `roundtable` CLI, so install the CLI first
(`curl -fsSL https://roundtable.sh/install.sh | bash`). Alternatively, `roundtable install`
auto-wires both the skill and the MCP server into Claude Code in one step.

## Keep it current

`roundtable version` · `roundtable update`.

---

*Roundtable — a Frontier Infra project. MIT. https://roundtable.sh ·
https://github.com/frontier-infra/roundtable*
