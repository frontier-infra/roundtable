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

From a clone: `git clone https://github.com/frontier-infra/roundtable && cd roundtable && ./install.sh --from .`

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
`--timeout <secs>`, `--out <file>`. Exit 0 if ≥1 head answered.

**Output discipline:** always return the per-head results table **and** the judge's
decision (chair verdict + your own moderator call). Never summarize without the table.

## Use as MCP / wire into your harness

Roundtable ships an MCP server as a subcommand of the one binary:

```bash
roundtable mcp serve     # stdio JSON-RPC; one tool: roundtable(question, heads?, rounds?, research?)
roundtable mcp config    # prints the server JSON block to paste manually
```

One line for Claude Code (needs the command line install):

```bash
claude mcp add --scope user roundtable -- roundtable mcp serve
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

```text
/plugin marketplace add frontier-infra/roundtable
/plugin install roundtable@frontier-infra
```

The plugin bundles everything: the `roundtable` command on the Bash tool's `PATH`, the
`roundtable` skill, `/roundtable:ask`, `/roundtable:setup`, and the `roundtable` MCP server.
No separate CLI install is needed. API keys entered in the plugin settings reach the MCP
server. The Bash command reads `~/.config/roundtable/config.env` instead.

## Keep it current

`roundtable version` · `roundtable update` (prints the right upgrade command for how you installed).

---

*Roundtable — a Frontier Infra project. MIT. https://roundtable.sh ·
https://github.com/frontier-infra/roundtable*
