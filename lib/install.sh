#!/usr/bin/env bash
# roundtable install — detect coding harnesses and wire Roundtable into each,
# IDEMPOTENTLY: back up any file before editing, merge (never clobber unrelated
# entries), and treat a re-run as a no-op.
#
# Usage:
#   install.sh [--harness a,b,c] [--all] [--print] [--use-claude-cli]
#
#   (no flags)        wire every harness detected by its config dir
#   --harness LIST    wire only the named harnesses (csv: claude,cursor,codex)
#   --all             wire all known harnesses regardless of detection
#   --print           don't write anything — just print the JSON + TOML to paste
#   --use-claude-cli  register the Claude Code MCP server via `claude mcp add`
#                     instead of editing the JSON config file. NOTE: the claude
#                     CLI ignores $HOME (it follows CLAUDE_CONFIG_DIR / the real
#                     home), so the default is a direct, $HOME-respecting JSON edit.
#
# Harness config paths (all keyed off $HOME so this is testable against a temp HOME):
#   Claude Code : skill -> <skills>/roundtable/SKILL.md ; MCP -> $HOME/.claude.json
#   Cursor      : $HOME/.cursor/mcp.json   (mcpServers.roundtable)
#   Codex CLI   : $HOME/.codex/config.toml ([mcp_servers.roundtable])
#
# Env: ROUNDTABLE_SKILL_SRC overrides the SKILL.md source path.
# python3 stdlib only; no new dependencies.
set -uo pipefail

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SELF_DIR/.." && pwd)"

# Resolve the SKILL.md to drop into Claude Code. Search order:
#   1) $ROUNDTABLE_SKILL_SRC                                  (explicit override)
#   2) plugins/roundtable/skills/roundtable/SKILL.md          (repo checkout — canonical source)
#   3) skill/SKILL.md                                         (engine-local copy shipped next to
#                                                              bin/+lib/ by the curl installer & pip wheel)
# Falls back to the canonical repo path (for the not-found warning) when none exist.
resolve_skill_src() {
  local c
  for c in \
    "${ROUNDTABLE_SKILL_SRC:-}" \
    "$REPO_ROOT/plugins/roundtable/skills/roundtable/SKILL.md" \
    "$REPO_ROOT/skill/SKILL.md"; do
    [[ -n "$c" && -f "$c" ]] && { printf '%s\n' "$c"; return 0; }
  done
  printf '%s\n' "${ROUNDTABLE_SKILL_SRC:-$REPO_ROOT/plugins/roundtable/skills/roundtable/SKILL.md}"
}
SKILL_SRC="$(resolve_skill_src)"

CMD="roundtable"
SERVER_KEY="roundtable"

CURSOR_CFG="$HOME/.cursor/mcp.json"
CODEX_CFG="$HOME/.codex/config.toml"
CLAUDE_CFG="$HOME/.claude.json"

info() { printf '  %s\n' "$*"; }
warn() { printf 'WARN: %s\n' "$*" >&2; }

usage() { sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

# ── shared JSON merge: ensure mcpServers.roundtable; backup before any edit ──
json_merge() {
  local f="$1"
  RT_CMD="$CMD" RT_KEY="$SERVER_KEY" python3 - "$f" <<'PY'
import json, os, sys, shutil
f = sys.argv[1]
key, cmd = os.environ["RT_KEY"], os.environ["RT_CMD"]
desired = {"command": cmd, "args": ["mcp", "serve"]}
existed = os.path.exists(f)
data = {}
if existed:
    try:
        with open(f) as fh:
            txt = fh.read().strip()
        data = json.loads(txt) if txt else {}
    except Exception as e:
        print("ERROR (invalid JSON, left untouched): %s" % e); sys.exit(3)
if not isinstance(data, dict):
    print("ERROR (top-level is not a JSON object, left untouched)"); sys.exit(3)
servers = data.get("mcpServers")
if not isinstance(servers, dict):
    servers = {}
if servers.get(key) == desired:
    print("UNCHANGED"); sys.exit(0)
os.makedirs(os.path.dirname(f) or ".", exist_ok=True)
if existed:
    shutil.copy2(f, f + ".bak")
servers[key] = desired
data["mcpServers"] = servers
with open(f, "w") as fh:
    json.dump(data, fh, indent=2)
    fh.write("\n")
print("CHANGED")
PY
}

# ── Codex TOML: append a clean [mcp_servers.roundtable] block iff absent ──
codex_toml() {
  local f="$1"
  RT_CMD="$CMD" RT_KEY="$SERVER_KEY" python3 - "$f" <<'PY'
import os, sys, shutil
f = sys.argv[1]
key, cmd = os.environ["RT_KEY"], os.environ["RT_CMD"]
existed = os.path.exists(f)
content = ""
present = False
if existed:
    with open(f) as fh:
        content = fh.read()
    try:
        import tomllib
        present = key in (tomllib.loads(content).get("mcp_servers") or {})
    except Exception:
        present = ("[mcp_servers.%s]" % key) in content  # fallback text scan
if present:
    print("UNCHANGED"); sys.exit(0)
os.makedirs(os.path.dirname(f) or ".", exist_ok=True)
if existed:
    shutil.copy2(f, f + ".bak")
sep = "" if (content == "" or content.endswith("\n")) else "\n"
block = '%s\n[mcp_servers.%s]\ncommand = "%s"\nargs = ["mcp", "serve"]\n' % (sep, key, cmd)
with open(f, "a") as fh:
    fh.write(block)
print("CHANGED")
PY
}

# ── Claude Code skill drop ──
pick_skills_dir() {
  local cand
  for cand in "$HOME/.agents/skills" "$HOME/.claude/skills"; do
    [[ -d "$cand" ]] && { printf '%s\n' "$cand"; return; }
  done
  for cand in "$HOME"/.claude*/skills; do
    [[ -d "$cand" ]] && { printf '%s\n' "$cand"; return; }
  done
  printf '%s\n' "$HOME/.claude/skills"   # sensible default if none present yet
}

drop_skill() {
  local dir target
  dir="$(pick_skills_dir)"
  target="$dir/roundtable/SKILL.md"
  if [[ ! -f "$SKILL_SRC" ]]; then
    warn "skill source not found at $SKILL_SRC — skipping skill drop (packaging adds it; MCP still wired)"
    return
  fi
  if [[ -f "$target" ]] && cmp -s "$SKILL_SRC" "$target"; then
    info "skill: UNCHANGED ($target)"
    return
  fi
  mkdir -p "$dir/roundtable"
  [[ -f "$target" ]] && cp -p "$target" "$target.bak"
  cp "$SKILL_SRC" "$target"
  info "skill: installed -> $target"
}

# ── per-harness wiring ──
wire_claude() {
  info "Claude Code:"
  drop_skill
  if [[ "$USE_CLAUDE_CLI" -eq 1 ]] && command -v claude >/dev/null 2>&1; then
    if claude mcp add "$SERVER_KEY" --scope user -- "$CMD" mcp serve >/dev/null 2>&1; then
      info "MCP: registered via 'claude mcp add' (writes to the CLI's own config, not \$HOME)"
    else
      warn "'claude mcp add' failed — falling back to JSON edit of $CLAUDE_CFG"
      info "MCP: $(json_merge "$CLAUDE_CFG") ($CLAUDE_CFG)"
    fi
  else
    info "MCP: $(json_merge "$CLAUDE_CFG") ($CLAUDE_CFG)"
  fi
}

wire_cursor() {
  info "Cursor:"
  info "MCP: $(json_merge "$CURSOR_CFG") ($CURSOR_CFG)"
}

wire_codex() {
  info "Codex CLI:"
  info "MCP: $(codex_toml "$CODEX_CFG") ($CODEX_CFG)"
}

print_blocks() {
  echo "── Paste into a JSON-based harness (Cursor, Claude Code, etc.) ──"
  python3 - <<'PY'
import json
print(json.dumps({"mcpServers": {"roundtable": {"command": "roundtable", "args": ["mcp", "serve"]}}}, indent=2))
PY
  echo
  echo "── Append to ~/.codex/config.toml (Codex CLI) ──"
  printf '[mcp_servers.roundtable]\ncommand = "roundtable"\nargs = ["mcp", "serve"]\n'
}

detect_present() {
  local present=()
  if [[ -d "$HOME/.agents/skills" ]] || compgen -G "$HOME/.claude*/skills" >/dev/null 2>&1 \
     || [[ -f "$CLAUDE_CFG" ]] || command -v claude >/dev/null 2>&1; then
    present+=(claude)
  fi
  [[ -d "$HOME/.cursor" ]] && present+=(cursor)
  [[ -d "$HOME/.codex" ]] && present+=(codex)
  printf '%s\n' "${present[*]:-}"
}

# ── arg parsing ──
HARNESSES=""
DO_ALL=0
PRINT_ONLY=0
USE_CLAUDE_CLI=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    install)            shift ;;                     # tolerate the dispatched verb
    --harness)          HARNESSES="${2:-}"; shift 2 ;;
    --harness=*)        HARNESSES="${1#*=}"; shift ;;
    --all)              DO_ALL=1; shift ;;
    --print|--print-only) PRINT_ONLY=1; shift ;;
    --use-claude-cli)   USE_CLAUDE_CLI=1; shift ;;
    -h|--help)          usage; exit 0 ;;
    *)                  warn "ignoring unknown arg: $1"; shift ;;
  esac
done

echo "🪑 roundtable install"

if [[ "$PRINT_ONLY" -eq 1 ]]; then
  print_blocks
  exit 0
fi

# resolve target harness list
if [[ -n "$HARNESSES" ]]; then
  IFS=',' read -ra TARGETS <<< "$HARNESSES"
elif [[ "$DO_ALL" -eq 1 ]]; then
  TARGETS=(claude cursor codex)
else
  read -ra TARGETS <<< "$(detect_present)"
fi

if [[ "${#TARGETS[@]}" -eq 0 ]]; then
  warn "no harnesses detected — printing config to paste manually:"
  print_blocks
  exit 0
fi

for h in "${TARGETS[@]}"; do
  case "$h" in
    claude) wire_claude ;;
    cursor) wire_cursor ;;
    codex)  wire_codex ;;
    generic|print) print_blocks ;;
    "")     ;;
    *)      warn "unknown harness '$h' (known: claude, cursor, codex)" ;;
  esac
done

echo "Done. Re-run any time — it's idempotent."
