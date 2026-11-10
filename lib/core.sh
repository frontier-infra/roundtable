#!/usr/bin/env bash
# core.sh — Roundtable multi-model COUNCIL with optional multi-round DELIBERATION to consensus.
# A Frontier Infra project.
#
# Heads: Grok · Codex (CLI) · OpenAI · GLM · MiniMax · Claude · Gemini — keyed REST APIs (+ optional local CLIs).
# Claude (Anthropic API) also acts as the CHAIR that judges consensus and synthesizes.
#
# MODES:
#   --rounds 1  (default)  ADVISORY     — each head answers once, BLIND to the others. Raw answers.
#   --rounds N  (N=2..4)   DELIBERATION — round 1 blind; rounds 2..N each head sees the WHOLE table
#                          and revises; the chair declares CONSENSUS or CONTINUE after each round and
#                          stops early on consensus. Prints every round + the chair's verdicts.
#                          At most 4 rounds.
#
# PERSONAS: --personas SPEC seats a role on each AI (lib/personas.py, packs in ../personas):
#   a preset (pricing), a pack (business-roles), or names (cfo,ceo@claude). generic = plain seats.
#
# CONFIG: keys/overrides load from ~/.config/roundtable/config.env (dotenv), then ~/.api_keys (back-compat).
# KEYS: XAI_API_KEY (Grok) · OPENAI_API_KEY (OpenAI) · ZAI_API_KEY (GLM) · MINIMAX_API_KEY ·
#       GEMINI_API_KEY/GOOGLE_API_KEY (Gemini) · ANTHROPIC_API_KEY (Claude head + chair).
#       Grok also falls back to a Hermes OAuth token then the local `grok` CLI; Codex uses the local `codex` CLI.
#
# USAGE:
#   core.sh -q "question"                            # advisory, all heads
#   core.sh -q "question" --rounds 3                 # deliberate up to 3 rounds, stop at consensus
#   core.sh --heads grok,glm,claude -q "..."         # subset of heads
#   core.sh -q "..." -c context.md                   # shared context file for every head
#   core.sh -q "..." --research                      # web ON + multi-step (current/external facts)
#   core.sh -q "..." --out /tmp/rt.md                # also save the transcript
#   core.sh -q "..." --personas pricing              # seat a panel of roles on the AIs
#   core.sh -q "..." --coach                         # the coach asks up to 3 questions, then writes a brief
#   core.sh -q "..." --verdict decision              # final verdict: decision|pros-cons|plan|vote|none (+ minority view)
#                                                    # a table of 2+ seats gets a decision by default; it leads the
#                                                    # transcript (pick, confidence, what would change it, 3 next steps)
#   core.sh -q "..." --no-clarify                    # skip the check for missing key facts (no brief: the chair
#                                                    # opens with 2 or 3 questions when the pick depends on them)
#   core.sh -q "..." --no-blind                      # round 1 open: each seat reads the seats before it
#   core.sh -q "..." --attach notes.pdf --attach URL # material every seat reads (long ones summarized once)
#   core.sh -q "..." --follow-up last.md             # a follow-up: the earlier transcript comes first
#
# MORE FLAGS: --brief FILE (a ready brief) · --coach-answer A (repeat, in order) · --coach-questions
#             (print the coach's questions and stop) · --attached FILE (material from `roundtable attach`) ·
#             --personas auto (suggest seats from the question or brief)
#
# ENV OVERRIDES: ROUNDTABLE_{GROK,OPENAI,GLM,MINIMAX,GEMINI,CLAUDE,CHAIR}_MODEL ·
#                ROUNDTABLE_{GROK_XAI,OPENAI,GLM,MINIMAX,GEMINI}_URL
# Exit 0 if at least one head answered.
set -uo pipefail

HEADS="grok,codex,openai,glm,minimax,claude,gemini"
ROUNDS=1
TIMEOUT=""
RESEARCH=0
CONTEXT_FILE=""
OUT_FILE=""
QUESTION=""
PERSONAS="${ROUNDTABLE_PERSONAS:-}"
COACH=0
COACH_QUESTIONS=0
declare -a COACH_ANSWERS ATTACH_SRCS
COACH_ANSWERS=(); ATTACH_SRCS=()
BRIEF_FILE=""
VERDICT=""
VERDICT_GIVEN=0
CLARIFY=1
OPEN_FIRST=0
FOLLOW_UP=""
ATTACHED_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -q|--question) QUESTION="$2"; shift 2 ;;
    -c|--context)  CONTEXT_FILE="$2"; shift 2 ;;
    --heads)       HEADS="$2"; shift 2 ;;
    --rounds)      ROUNDS="$2"; shift 2 ;;
    --personas)    PERSONAS="$2"; shift 2 ;;
    --timeout)     TIMEOUT="$2"; shift 2 ;;
    --research)    RESEARCH=1; shift ;;
    --out)         OUT_FILE="$2"; shift 2 ;;
    --coach)       COACH=1; shift ;;
    --coach-answer) COACH=1; COACH_ANSWERS+=("$2"); shift 2 ;;
    --coach-questions) COACH_QUESTIONS=1; shift ;;
    --brief)       BRIEF_FILE="$2"; shift 2 ;;
    --verdict)     VERDICT="$2"; VERDICT_GIVEN=1; shift 2 ;;
    --no-clarify)  CLARIFY=0; shift ;;
    --no-blind|--open-first-round) OPEN_FIRST=1; shift ;;
    --blind)       OPEN_FIRST=0; shift ;;
    --follow-up)   FOLLOW_UP="$2"; shift 2 ;;
    --attach)      ATTACH_SRCS+=("$2"); shift 2 ;;
    --attached)    ATTACHED_FILE="$2"; shift 2 ;;
    -h|--help)     grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)             QUESTION="${QUESTION:+$QUESTION }$1"; shift ;;
  esac
done

[[ -z "$TIMEOUT" ]] && { if [[ "$RESEARCH" -eq 1 ]]; then TIMEOUT=360; else TIMEOUT=300; fi; }
[[ -z "$QUESTION" ]] && { echo "ERROR: no question. Use -q \"...\"." >&2; exit 2; }
[[ "$ROUNDS" =~ ^[0-9]+$ ]] || { echo "ERROR: --rounds needs a number" >&2; exit 2; }
[[ "$ROUNDS" -lt 1 ]] && ROUNDS=1
[[ "$ROUNDS" -gt 4 ]] && { echo "NOTE: --rounds is at most 4. Running 4." >&2; ROUNDS=4; }
case "$(printf '%s' "$VERDICT" | tr '[:upper:]' '[:lower:]')" in
  ""|none|off) VERDICT="" ;;
  decision) VERDICT="decision" ;;
  pros-cons|pros_cons|proscons|pros) VERDICT="pros-cons" ;;
  plan|action-plan|action) VERDICT="plan" ;;
  vote) VERDICT="vote" ;;
  *) echo "ERROR: Unknown verdict '$VERDICT'. Use decision, pros-cons, plan or vote." >&2; exit 2 ;;
esac

TIMEOUT_BIN=""
command -v gtimeout >/dev/null 2>&1 && TIMEOUT_BIN="gtimeout"
[[ -z "$TIMEOUT_BIN" ]] && command -v timeout >/dev/null 2>&1 && TIMEOUT_BIN="timeout"
run_to() { if [[ -n "$TIMEOUT_BIN" ]]; then "$TIMEOUT_BIN" "$@"; else shift; "$@"; fi; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/roundtable.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
PFILE="$WORK/round.prompt"

# Config loader — source the generic config first, then ~/.api_keys (back-compat). Both optional.
# Called once up front (so ROUNDTABLE_*_MODEL/URL overrides in config.env feed the MODELS block
# below) and again inside each head (so keys are present even if the environment was reset).
load_keys() {
  set +u
  [[ -f "$HOME/.config/roundtable/config.env" ]] && . "$HOME/.config/roundtable/config.env" 2>/dev/null
  [[ -f "$HOME/.api_keys" ]] && . "$HOME/.api_keys" 2>/dev/null
  set -u
}
load_keys

# ══════════════════════════════════════════════════════════════════════════
#  MODELS — EDIT HERE to change which model each head uses.
#  Models ship fast and go stale; update the id after the := as new ones land.
#  Each is also overridable at runtime via its ROUNDTABLE_*_MODEL env var
#  (set it in ~/.config/roundtable/config.env to persist).
#  (date in comment = last time the id was checked / updated)
# ══════════════════════════════════════════════════════════════════════════
GROK_MODEL="${ROUNDTABLE_GROK_MODEL:-grok-4.3}"                     # 🤖 xAI Grok — api.x.ai seat (XAI_API_KEY; Hermes OAuth fallback)
GROK_CLI_MODEL="${ROUNDTABLE_GROK_CLI_MODEL:-}" # 🤖 grok CLI fallback (used only if no key/token); empty = the CLI's own default model
OPENAI_MODEL="${ROUNDTABLE_OPENAI_MODEL:-gpt-5.5}"                  # 🧠 OpenAI — keyed seat (api.openai.com)
GLM_MODEL="${ROUNDTABLE_GLM_MODEL:-glm-5.2}"                        # 🟣 Z.AI GLM
MINIMAX_MODEL="${ROUNDTABLE_MINIMAX_MODEL:-MiniMax-M3}"             # 🟠 MiniMax
GEMINI_MODEL="${ROUNDTABLE_GEMINI_MODEL:-gemini-3.1-pro-preview}"   # ✨ Google Gemini — updated 2026-06-17 (Gemini 3.1 Pro)
CLAUDE_MODEL="${ROUNDTABLE_CLAUDE_MODEL:-claude-opus-4-8}"          # 🔵 Anthropic — Claude head seat
CHAIR_MODEL="${ROUNDTABLE_CHAIR_MODEL:-claude-opus-4-8}"            # 🪑 Anthropic — consensus chair/judge
# 🧠 Codex head uses the local `codex` CLI's own default model — no id to set here.

# ── API endpoints (rarely change; edit only if a provider moves its URL) ──
GROK_XAI_URL="${ROUNDTABLE_GROK_XAI_URL:-https://api.x.ai/v1/chat/completions}"
OPENAI_URL="${ROUNDTABLE_OPENAI_URL:-https://api.openai.com/v1/chat/completions}"
GLM_URL="${ROUNDTABLE_GLM_URL:-https://api.z.ai/api/coding/paas/v4/chat/completions}"
MINIMAX_URL="${ROUNDTABLE_MINIMAX_URL:-https://api.minimax.io/v1/chat/completions}"
GEMINI_URL="${ROUNDTABLE_GEMINI_URL:-https://generativelanguage.googleapis.com/v1beta/openai/chat/completions}"

# Display labels read the live MODEL vars so the header never drifts from the
# actual model in use — shows e.g. "✨ Gemini (gemini-3.1-pro-preview)".
label_for() {
  case "$1" in
    grok) echo "🤖 Grok ($GROK_MODEL, xAI)";;
    codex) echo "🧠 Codex / GPT (OpenAI CLI)";;
    openai) echo "🧠 OpenAI ($OPENAI_MODEL)";;
    glm) echo "🟣 GLM ($GLM_MODEL, Z.AI)";;
    minimax) echo "🟠 MiniMax ($MINIMAX_MODEL)";;
    claude) echo "🔵 Claude ($CLAUDE_MODEL, Anthropic)";;
    gemini) echo "✨ Gemini ($GEMINI_MODEL, Google)";;
    *) echo "$1";;
  esac
}

# ── SEAT ISOLATION (same rules as desktop/engine/src/clis.rs) ──
# A CLI seat answers from what it knows, never from this computer: a fresh empty folder,
# no tools, web off (unless --research), no MCP servers, and the tool's own instructions
# and memory off where it has a switch. A failed run is asked once more, unless it failed
# on sign-in or a usage limit.
seat_rule() {
  if [[ "$RESEARCH" -eq 1 ]]; then
    printf '%s' "You answer as part of an advisory council. Your only tools are web search and web fetch: do not read, open or look up any file, skill or command on this computer. Answer in plain Markdown, in this one reply. Earlier rules that ask you to read a file or use another tool do not apply here."
  else
    printf '%s' "You answer as part of an advisory council. You have no tools: do not read, open or look up any file, skill or command, and do not search the web. Answer from what you know, in plain Markdown, in this one reply. Earlier rules that ask you to read a file or use a tool do not apply here."
  fi
}
GROK_NO_TOOLS="todo_write,search_tool,use_tool,CallMcpTool"
GROK_ENV=(GROK_MEMORY=0 GROK_WORKFLOWS=0 GROK_DISABLE_AUTOUPDATER=1 GROK_CLAUDE_AGENTS_ENABLED=0 GROK_CLAUDE_RULES_ENABLED=0
  GROK_CLAUDE_SKILLS_ENABLED=0 GROK_CLAUDE_MCPS_ENABLED=0 GROK_CLAUDE_HOOKS_ENABLED=0 GROK_CURSOR_AGENTS_ENABLED=0
  GROK_CURSOR_RULES_ENABLED=0 GROK_CURSOR_SKILLS_ENABLED=0 GROK_CURSOR_MCPS_ENABLED=0 GROK_CURSOR_HOOKS_ENABLED=0)
CODEX_OFF=(shell_tool code_mode_host memories apps plugins hooks browser_use computer_use in_app_browser multi_agent multi_agent_v2 skill_search tool_suggest sleep_tool)
# seat_dir: a fresh empty folder for one CLI run (printed).
seat_dir() { local d; d="$(mktemp -d "$WORK/seat.XXXXXX")"; printf '%s' "$d"; }
# seat_retryable <rc> <errfile>: true when one more try may help.
seat_retryable() {
  local t; t="$(tr '[:upper:]' '[:lower:]' < "$2" 2>/dev/null | tail -c 2000)"
  case "$t" in *"not logged in"*|*"not signed in"*|*"login required"*|*"unauthorized"*|*"401"*|*"usage limit"*|*"rate limit"*|*"quota"*|*"unknown option"*|*"unexpected argument"*) return 1;; esac
  return 0
}

run_grok() {
  local out="$WORK/grok.out" err="$WORK/grok.err" body="$WORK/grok.body" resp="$WORK/grok.resp"
  load_keys
  # Preferred: XAI_API_KEY → api.x.ai (grok-4.3). Optional fallback: a Hermes SuperGrok OAuth
  # token (~/.hermes/auth.json), used only if XAI_API_KEY is unset. Last resort: the local grok CLI.
  local tok="${XAI_API_KEY:-}"
  if [[ -z "$tok" ]]; then
    tok="$(python3 -c "import json,os
try:
  d=json.load(open(os.path.expanduser('~/.hermes/auth.json')))
except Exception: raise SystemExit
def walk(o,p=''):
  r=[]
  if isinstance(o,dict):
    if 'access_token' in o: r.append((p,o['access_token']))
    for k,v in o.items(): r+=walk(v,p+'/'+str(k))
  elif isinstance(o,list):
    for i,v in enumerate(o): r+=walk(v,p+'['+str(i)+']')
  return r
ts=walk(d); xa=[t for p,t in ts if 'xai' in p.lower() or 'grok' in p.lower()]
import sys; sys.stdout.write((xa[0] if xa else '') or '')
" 2>/dev/null)"
  fi
  if [[ -n "$tok" ]]; then
    GROK_MODEL="$GROK_MODEL" python3 - "$PFILE" >"$body" <<'PY'
import os,sys,json
p=open(sys.argv[1]).read()
print(json.dumps({"model":os.environ["GROK_MODEL"],"messages":[{"role":"user","content":p}],"max_tokens":6000,"temperature":0.6,"stream":False}))
PY
    run_to "$TIMEOUT" curl -sS "$GROK_XAI_URL" -H "Authorization: Bearer $tok" -H "Content-Type: application/json" --data @"$body" >"$resp" 2>"$err"
    local got
    got="$(python3 -c "import sys,json
try:
 d=json.load(open('$resp')); sys.stdout.write((d['choices'][0]['message'].get('content') or '').strip())
except Exception: pass" 2>/dev/null)"
    [[ -n "$got" ]] && { printf '%s' "$got" >"$out"; return; }
  fi
  # Fallback: grok CLI (coding-plan composer), isolated like every CLI seat (see SEAT ISOLATION).
  command -v grok >/dev/null 2>&1 || { echo "__MISSING__ (no XAI_API_KEY / OAuth token / grok CLI)" >"$out"; return; }
  local p; p="$(cat "$PFILE")"
  local gm=(); [[ -n "$GROK_CLI_MODEL" ]] && gm=(-m "$GROK_CLI_MODEL")
  local ga
  if [[ "$RESEARCH" -eq 1 ]]; then ga=(--max-turns 16 --tools web_search,web_fetch --no-subagents --rules "$(seat_rule)")
  else ga=(--tools todo_write --disallowed-tools "$GROK_NO_TOOLS" --no-subagents --disable-web-search --rules "$(seat_rule)"); fi
  local rc try
  for try in 1 2; do
    ( cd "$(seat_dir)" && env "${GROK_ENV[@]}" "$(command -v grok)" -p "$p" ${gm[@]+"${gm[@]}"} "${ga[@]}" </dev/null >"$out" 2>"$err" )
    rc=$?
    [[ $rc -eq 0 && -s "$out" ]] && return
    seat_retryable "$rc" "$err" || break
  done
  [[ $rc -eq 124 ]] && { echo "__TIMEOUT__ (${TIMEOUT}s)" >"$out"; return; }
  [[ ! -s "$out" || $rc -ne 0 ]] && echo "__EMPTY__ (rc=$rc) $(tail -c 200 "$err" 2>/dev/null)" >"$out"
}

run_codex() {
  local out="$WORK/codex.out" err="$WORK/codex.err" last="$WORK/codex.last"
  command -v codex >/dev/null 2>&1 || { echo "__MISSING__" >"$out"; return; }
  local p; p="$(cat "$PFILE")"
  local ca=(exec --skip-git-repo-check --ephemeral --ignore-rules -s read-only -c project_doc_max_bytes=0) f
  for f in "${CODEX_OFF[@]}"; do ca+=(-c "features.$f=false"); done
  [[ "$RESEARCH" -eq 1 ]] || ca+=(-c tools.web_search=false -c 'web_search="disabled"')
  # A clean CODEX_HOME: the sign-in file linked, the settings filtered to the model and provider.
  local src="${CODEX_HOME:-$HOME/.codex}" ch="$WORK/codex-home" envs=()
  if [[ -f "$src/auth.json" ]]; then
    mkdir -p "$ch" && chmod 700 "$ch" && ln -sf "$src/auth.json" "$ch/auth.json"
    ( umask 077; python3 "$COACH_PY" codex-config "$src/config.toml" > "$ch/config.toml" )
    envs=(CODEX_HOME="$ch")
  else
    ca=(exec --ignore-user-config "${ca[@]:1}")
  fi
  local rc try
  for try in 1 2; do
    rm -f "$last"
    ( cd "$(seat_dir)" && env ${envs[@]+"${envs[@]}"} "$(command -v codex)" "${ca[@]}" -o "$last" "$p" >"$err" 2>&1 )
    rc=$?
    [[ -s "$last" ]] && { cp "$last" "$out"; rm -rf "$ch"; return; }
    seat_retryable "$rc" "$err" || break
  done
  rm -rf "$ch"
  [[ $rc -eq 124 ]] && { echo "__TIMEOUT__ (${TIMEOUT}s)" >"$out"; return; }
  echo "__EMPTY__ (rc=$rc) $(tail -c 200 "$err" 2>/dev/null)" >"$out"
}

run_openai() {
  local out="$WORK/openai.out" err="$WORK/openai.err" body="$WORK/openai.body" resp="$WORK/openai.resp"
  load_keys; local key="${OPENAI_API_KEY:-}"
  [[ -z "$key" ]] && { echo "__MISSING__ (no OPENAI_API_KEY)" >"$out"; return; }
  OPENAI_MODEL="$OPENAI_MODEL" python3 - "$PFILE" >"$body" <<'PY'
import os,sys,json
p=open(sys.argv[1]).read()
print(json.dumps({"model":os.environ["OPENAI_MODEL"],"messages":[{"role":"user","content":p}],
  "max_tokens":8000,"temperature":0.6,"stream":False}))
PY
  run_to "$TIMEOUT" curl -sS "$OPENAI_URL" -H "Authorization: Bearer $key" -H "Content-Type: application/json" --data @"$body" >"$resp" 2>"$err"
  local got
  got="$(python3 -c "import sys,json
try:
 d=json.load(open('$resp')); sys.stdout.write((d['choices'][0]['message'].get('content') or '').strip())
except Exception: pass" 2>/dev/null)"
  if [[ -n "$got" ]]; then printf '%s' "$got" >"$out"; else echo "__EMPTY__ (openai: $(tail -c 160 "$resp" 2>/dev/null))" >"$out"; fi
}

run_glm() {
  local out="$WORK/glm.out" err="$WORK/glm.err" body="$WORK/glm.body" resp="$WORK/glm.resp"
  load_keys; local key="${ZAI_API_KEY:-}" altkey="${ZAI_API_KEY_JASON:-}"
  [[ -z "${key}${altkey}" ]] && { echo "__MISSING__ (no ZAI_API_KEY)" >"$out"; return; }
  GLM_MODEL="$GLM_MODEL" RESEARCH="$RESEARCH" python3 - "$PFILE" >"$body" <<'PY'
import os,sys,json
p=open(sys.argv[1]).read()
print(json.dumps({"model":os.environ["GLM_MODEL"],"messages":[{"role":"user","content":p}],
  "max_tokens":6000,"temperature":0.6,"stream":False,
  "thinking":{"type":"enabled" if os.environ.get("RESEARCH")=="1" else "disabled"}}))
PY
  local got=""
  for K in "$key" "$altkey"; do
    [[ -z "$K" ]] && continue
    run_to "$TIMEOUT" curl -sS "$GLM_URL" -H "Authorization: Bearer $K" -H "Content-Type: application/json" --data @"$body" >"$resp" 2>"$err"
    got="$(python3 -c "import sys,json
try:
 d=json.load(open('$resp')); sys.stdout.write((d['choices'][0]['message'].get('content') or '').strip())
except Exception: pass" 2>/dev/null)"
    [[ -n "$got" ]] && { printf '%s' "$got" >"$out"; return; }
  done
  echo "__EMPTY__ (glm: $(tail -c 160 "$resp" 2>/dev/null))" >"$out"
}

run_minimax() {
  local out="$WORK/minimax.out" err="$WORK/minimax.err" body="$WORK/minimax.body" resp="$WORK/minimax.resp"
  load_keys; local key="${MINIMAX_API_KEY:-}"
  [[ -z "$key" ]] && { echo "__MISSING__ (no MINIMAX_API_KEY)" >"$out"; return; }
  MINIMAX_MODEL="$MINIMAX_MODEL" python3 - "$PFILE" >"$body" <<'PY'
import os,sys,json
p=open(sys.argv[1]).read()
print(json.dumps({"model":os.environ["MINIMAX_MODEL"],"messages":[{"role":"user","content":p}],
  "max_tokens":8000,"temperature":0.6,"stream":False}))
PY
  run_to "$TIMEOUT" curl -sS "$MINIMAX_URL" -H "Authorization: Bearer $key" -H "Content-Type: application/json" --data @"$body" >"$resp" 2>"$err"
  local got
  got="$(python3 -c "import sys,json,re
try:
 d=json.load(open('$resp')); c=d['choices'][0]['message'].get('content') or ''
 sys.stdout.write(re.sub(r'<think>.*?</think>','',c,flags=re.S).strip())
except Exception: pass" 2>/dev/null)"
  if [[ -n "$got" ]]; then printf '%s' "$got" >"$out"; else echo "__EMPTY__ (minimax: $(tail -c 160 "$resp" 2>/dev/null))" >"$out"; fi
}

# anthropic_call <model> <promptfile> -> prints text  (claude head + chair)
anthropic_call() {
  local model="$1" pfile="$2" body resp; body="$(mktemp)"; resp="$(mktemp)"
  load_keys; local ak="${ANTHROPIC_API_KEY:-}"
  [[ -z "$ak" ]] && { rm -f "$body" "$resp"; return 1; }
  MODEL="$model" python3 - "$pfile" >"$body" <<'PY'
import os,sys,json
p=open(sys.argv[1]).read()
print(json.dumps({"model":os.environ["MODEL"],"max_tokens":8000,"messages":[{"role":"user","content":p}]}))
PY
  run_to "$TIMEOUT" curl -sS "${ROUNDTABLE_ANTHROPIC_URL:-https://api.anthropic.com/v1/messages}" \
    -H "x-api-key: $ak" -H "anthropic-version: 2023-06-01" -H "content-type: application/json" \
    --data @"$body" >"$resp" 2>/dev/null
  python3 -c "import sys,json
try:
 d=json.load(open('$resp')); sys.stdout.write(''.join(b.get('text','') for b in d.get('content',[])).strip())
except Exception: pass" 2>/dev/null
  rm -f "$body" "$resp"
}

run_gemini() {
  local out="$WORK/gemini.out" err="$WORK/gemini.err" body="$WORK/gemini.body" resp="$WORK/gemini.resp"
  load_keys; local key="${GEMINI_API_KEY:-${GOOGLE_API_KEY:-}}"
  [[ -z "$key" ]] && { echo "__MISSING__ (no GEMINI_API_KEY/GOOGLE_API_KEY)" >"$out"; return; }
  GEMINI_MODEL="$GEMINI_MODEL" python3 - "$PFILE" >"$body" <<'PY'
import os,sys,json
p=open(sys.argv[1]).read()
# Gemini Pro is a thinking model: reasoning tokens count against max_tokens,
# so budget high or the visible answer gets truncated (finish_reason=length, empty content).
print(json.dumps({"model":os.environ["GEMINI_MODEL"],"messages":[{"role":"user","content":p}],
  "max_tokens":16000,"temperature":0.6,"stream":False}))
PY
  run_to "$TIMEOUT" curl -sS "$GEMINI_URL" -H "Authorization: Bearer $key" -H "Content-Type: application/json" --data @"$body" >"$resp" 2>"$err"
  local got
  got="$(python3 -c "import sys,json
try:
 d=json.load(open('$resp')); sys.stdout.write((d['choices'][0]['message'].get('content') or '').strip())
except Exception: pass" 2>/dev/null)"
  if [[ -n "$got" ]]; then printf '%s' "$got" >"$out"; else echo "__EMPTY__ (gemini: $(tail -c 160 "$resp" 2>/dev/null))" >"$out"; fi
}

run_claude() {
  local out="$WORK/claude.out" got
  got="$(anthropic_call "$CLAUDE_MODEL" "$PFILE")"
  if [[ -n "$got" ]]; then printf '%s' "$got" >"$out"; else echo "__EMPTY__ (claude api — check ANTHROPIC_API_KEY)" >"$out"; fi
}

# ── COACH, FOLLOW-UP, ATTACHMENTS, BRIEF (lib/coach.py builds the text; the chair answers) ──
COACH_PY="$(dirname "$0")/coach.py"
# chair_say <promptfile>: the chair's answer, or nothing.
chair_say() { local t; t="$TIMEOUT"; TIMEOUT=180; anthropic_call "$CHAIR_MODEL" "$1"; TIMEOUT="$t"; }

if [[ "$COACH_QUESTIONS" -eq 1 ]]; then
  python3 "$COACH_PY" questions-prompt "$QUESTION" > "$WORK/coach.prompt"
  _r="$(chair_say "$WORK/coach.prompt")"
  [[ -z "$_r" ]] && { echo "ERROR: The coach did not answer. Connect Claude, or skip the coach." >&2; exit 1; }
  printf '%s' "$_r" | python3 "$COACH_PY" parse-questions
  exit 0
fi

BRIEF=""
if [[ -n "$BRIEF_FILE" ]]; then
  [[ -f "$BRIEF_FILE" ]] || { echo "ERROR: could not read the brief file: $BRIEF_FILE" >&2; exit 2; }
  BRIEF="$(python3 -c 'import sys; print(open(sys.argv[1], encoding="utf-8").read().strip(), end="")' "$BRIEF_FILE")"
fi
if [[ "$COACH" -eq 1 && -z "$BRIEF" ]]; then
  python3 "$COACH_PY" questions-prompt "$QUESTION" > "$WORK/coach.prompt"
  _r="$(chair_say "$WORK/coach.prompt")"
  if [[ -z "$_r" ]]; then
    echo "NOTE: The coach did not answer. Connect Claude, or skip the coach. Asking the question as typed." >&2
  else
    _tty=0; [[ -t 0 && -t 2 ]] && _tty=1
    : > "$WORK/coach.qa"; _i=0
    while IFS= read -r _q; do
      [[ "$_q" == "NONE" || -z "$_q" ]] && continue
      _q="${_q#Q: }"
      if [[ $_i -lt ${#COACH_ANSWERS[@]} ]]; then _a="${COACH_ANSWERS[$_i]}"
      elif [[ $_tty -eq 1 ]]; then printf 'Coach: %s\n> ' "$_q" >&2; IFS= read -r _a </dev/tty || _a=""
      else _a=""; fi
      printf '%s\t%s\n' "$_q" "$(printf '%s' "$_a" | tr '\t\n' '  ')" >> "$WORK/coach.qa"
      _i=$((_i+1))
    done < <(printf '%s' "$_r" | python3 "$COACH_PY" parse-questions)
    python3 "$COACH_PY" brief-prompt "$QUESTION" "$WORK/coach.qa" > "$WORK/brief.prompt"
    _r="$(chair_say "$WORK/brief.prompt")"
    if [[ -z "$_r" ]]; then
      echo "NOTE: The coach did not answer. Connect Claude, or skip the coach. Asking the question as typed." >&2
    else
      BRIEF="$(printf '%s' "$_r" | python3 "$COACH_PY" parse-brief "$QUESTION")"
      if [[ $_tty -eq 1 ]]; then
        printf '\nThe brief the table will read:\n%s\n\nPress Enter to use it, or type s and Enter to skip it: ' "$BRIEF" >&2
        IFS= read -r _a </dev/tty || _a=""
        [[ "$_a" == "s" || "$_a" == "S" ]] && BRIEF=""
      fi
    fi
  fi
fi

PRIOR=""
if [[ -n "$FOLLOW_UP" ]]; then
  [[ -f "$FOLLOW_UP" ]] || { echo "ERROR: could not read the follow-up file: $FOLLOW_UP" >&2; exit 2; }
  PRIOR="$(cat "$FOLLOW_UP")"
fi

ATTACHMENTS=""
if [[ -n "$ATTACHED_FILE" ]]; then
  [[ -f "$ATTACHED_FILE" ]] || { echo "ERROR: could not read the attached file: $ATTACHED_FILE" >&2; exit 2; }
  ATTACHMENTS="$(python3 -c 'import sys; print(open(sys.argv[1], encoding="utf-8").read().strip(), end="")' "$ATTACHED_FILE")"
fi
if [[ ${#ATTACH_SRCS[@]} -gt 0 ]]; then
  _dirs=(); _n=0
  for _src in "${ATTACH_SRCS[@]}"; do
    [[ -z "${_src// /}" ]] && continue
    _d="$WORK/att$_n"; mkdir -p "$_d"; _n=$((_n+1))
    _len="$(python3 "$COACH_PY" attach-read "$_src" "$_d")" || exit 2
    if [[ "$_len" == "long" ]]; then
      python3 "$COACH_PY" summary-prompt "$(cat "$_d/name")" "$_d/text" > "$_d/summary.prompt"
      chair_say "$_d/summary.prompt" > "$_d/summary"
    fi
    python3 "$COACH_PY" attach-share "$_d" "$_d/summary" || exit 2
    _dirs+=("$_d")
  done
  if [[ ${#_dirs[@]} -gt 0 ]]; then
    _blk="$(python3 "$COACH_PY" block "${_dirs[@]}")"
    if [[ -n "$ATTACHMENTS" ]]; then ATTACHMENTS="$ATTACHMENTS"$'\n\n'"$_blk"; else ATTACHMENTS="$_blk"; fi
  fi
fi

# The shared prompt: earlier turns, background, attachments and the brief, then the question.
_ctx=""; [[ -n "$CONTEXT_FILE" && -f "$CONTEXT_FILE" ]] && _ctx="$(cat "$CONTEXT_FILE")"
BASE_PROMPT=""
_part() { [[ -z "${2//[[:space:]]/}" ]] && return; [[ -n "$BASE_PROMPT" ]] && BASE_PROMPT+=$'\n\n'; BASE_PROMPT+="## $1"$'\n'"$2"; }
_part "EARLIER IN THIS SESSION" "$PRIOR"
_part "CONTEXT" "$_ctx"
_part "ATTACHMENTS" "$ATTACHMENTS"
_part "BRIEF" "$BRIEF"

declare -a HARR ACTIVE
IFS=',' read -ra HARR <<< "$HEADS"
ACTIVE=()
for h in "${HARR[@]}"; do
  case "$h" in grok|codex|openai|glm|minimax|claude|gemini) ACTIVE+=("$h");; *) echo "WARN: unknown head '$h'" >&2;; esac
done
# Guard the empty case: bash 3.2 + `set -u` aborts on "${ACTIVE[@]}" when the array is empty,
# so a bad --heads list would otherwise crash. Skip gracefully with a clear message instead.
[[ ${#ACTIVE[@]} -eq 0 ]] && { echo "ERROR: no valid heads selected (after dropping unknown heads)." >&2; exit 2; }

# ── SEATS: one per head (Generic), or the personas from --personas on those heads ──
declare -a SEAT_HEAD SEAT_NAME SEAT_DISC
SEAT_HEAD=(); SEAT_NAME=(); SEAT_DISC=()
PERSONA_SEATS=0
_p="$(printf '%s' "$PERSONAS" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
if [[ "$_p" == "auto" ]]; then
  _text="${BRIEF:-$QUESTION}"
  _reply=""
  if [[ "$(printf '%s' "${ROUNDTABLE_SUGGEST:-}" | tr '[:upper:]' '[:lower:]')" != "words" ]]; then
    python3 "$COACH_PY" suggest-prompt "$_text" > "$WORK/suggest.prompt"
    _reply="$(chair_say "$WORK/suggest.prompt")"
  fi
  PERSONAS="$(printf '%s' "$_reply" | python3 "$COACH_PY" suggest-pick "$_text")"
  _p="$(printf '%s' "$PERSONAS" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
fi
if [[ -n "$_p" && "$_p" != "generic" ]]; then
  _csv="$(IFS=','; echo "${ACTIVE[*]}")"
  python3 "$(dirname "$0")/personas.py" seats "$PERSONAS" "$_csv" "$WORK" || exit 2
  while IFS=$'\t' read -r _h _n _d; do
    SEAT_HEAD+=("$_h"); [[ "$_n" == "-" ]] && _n=""; [[ "$_d" == "-" ]] && _d=""
    SEAT_NAME+=("$_n"); SEAT_DISC+=("$_d")
    [[ -n "$_n" ]] && PERSONA_SEATS=1
  done < "$WORK/seats.tsv"
else
  for h in "${ACTIVE[@]}"; do SEAT_HEAD+=("$h"); SEAT_NAME+=(""); SEAT_DISC+=(""); done
fi
NSEATS=${#SEAT_HEAD[@]}

# seat_label <i>: "CFO · 🔵 Claude (...)" on a persona seat, the head label on a Generic one.
seat_label() {
  local i="$1"
  if [[ -n "${SEAT_NAME[$i]}" ]]; then echo "${SEAT_NAME[$i]} · $(label_for "${SEAT_HEAD[$i]}")"; else label_for "${SEAT_HEAD[$i]}"; fi
}

# Every seat runs in its own subshell and folder, so two seats on one AI never share files.
# Each seat reads its brief (empty on a Generic seat), then the round's prompt.
run_all_seats() {
  local i
  for ((i=0; i<NSEATS; i++)); do
    ( WORK="$WORK/s$i"; mkdir -p "$WORK"; PFILE="$WORK/round.prompt"
      { [[ -f "$WORK/../brief.$i" ]] && cat "$WORK/../brief.$i"; cat "$ROUND_PFILE"; } > "$PFILE"
      "run_${SEAT_HEAD[$i]}" ) &
  done
  wait
}
ROUND_PFILE="$PFILE"

# answered <file>: true when the file holds a real answer (not a skip, timeout or empty sentinel).
answered() { [[ -s "$1" ]] && ! grep -q '^__\(MISSING\|TIMEOUT\|EMPTY\)__' <(head -c 12 "$1"); }

# --no-blind: round 1 runs seat by seat; each seat also reads the answers before it.
run_open_seats() {
  local i j
  for ((i=0; i<NSEATS; i++)); do
    mkdir -p "$WORK/s$i"
    { [[ -f "$WORK/brief.$i" ]] && cat "$WORK/brief.$i"
      cat "$ROUND_PFILE"
      _seen=0
      for ((j=0; j<i; j++)); do
        answered "$WORK/s$j/${SEAT_HEAD[$j]}.out" || continue
        if [[ $_seen -eq 0 ]]; then printf '\n\n---\n## ANSWERS SO FAR (round 1 is open): the seats that answered before you:\n\n'; _seen=1; fi
        printf '### %s\n%s\n\n' "$(seat_label "$j")" "$(cat "$WORK/s$j/${SEAT_HEAD[$j]}.out")"
      done
      [[ $_seen -eq 1 ]] && printf '## YOUR TASK\nGive your own answer. You may build on these answers or disagree with them; say which.\n'
    } > "$WORK/s$i/round.prompt"
    ( WORK="$WORK/s$i"; PFILE="$WORK/round.prompt"; "run_${SEAT_HEAD[$i]}" )
  done
}
first_round() { if [[ "$OPEN_FIRST" -eq 1 ]]; then run_open_seats; else run_all_seats; fi; }

# A table of two seats or more gets a decision verdict unless --verdict says otherwise.
[[ "$VERDICT_GIVEN" -eq 0 && "$NSEATS" -ge 2 ]] && VERDICT="decision"

# Clarify first: with no brief and no earlier turns, the chair checks the question for missing
# key facts. When the pick flips on them, the seats and the verdict open with its questions.
: > "$WORK/open.q"
if [[ "$CLARIFY" -eq 1 && "$NSEATS" -ge 2 && -z "${BRIEF//[[:space:]]/}" && -z "${PRIOR//[[:space:]]/}" ]]; then
  python3 "$COACH_PY" clarify-prompt "$QUESTION" > "$WORK/clarify.prompt"
  _t="$TIMEOUT"; [[ "$TIMEOUT" -gt 180 ]] && TIMEOUT=180
  anthropic_call "$CHAIR_MODEL" "$WORK/clarify.prompt" | python3 "$COACH_PY" clarify-questions > "$WORK/open.q"
  TIMEOUT="$_t"
fi
_part "OPEN QUESTIONS" "$(python3 "$COACH_PY" open-section "$WORK/open.q")"
if [[ -n "$BASE_PROMPT" ]]; then BASE_PROMPT+=$'\n\n'"## QUESTION"$'\n'"$QUESTION"; else BASE_PROMPT="$QUESTION"; fi

# positions <suffix>: every seat's answer as "### label\nbody\n\n" blocks (suffix "/" = this round's .out).
positions() {
  local i f
  for ((i=0; i<NSEATS; i++)); do
    f="$WORK/s$i$1"; [[ "$1" == "/" ]] && f="$WORK/s$i/${SEAT_HEAD[$i]}.out"
    printf '### %s\n%s\n\n' "$(seat_label "$i")" "$(body_of_file "$f")"
  done
}

# verdict_text <file suffix>: the chair's final verdict in the --verdict shape, into $WORK/verdict.md.
verdict_text() {
  local title text
  case "$VERDICT" in
    decision) title="decision" ;; pros-cons) title="pros and cons" ;; plan) title="action plan" ;; vote) title="vote" ;;
  esac
  positions "$1" > "$WORK/final.pos"
  printf '%s' "$BRIEF" > "$WORK/brief.md"
  python3 "$COACH_PY" verdict-prompt "$VERDICT" "$QUESTION" "$WORK/brief.md" "$WORK/open.q" "$WORK/final.pos" > "$WORK/verdict.prompt"
  text="$(anthropic_call "$CHAIR_MODEL" "$WORK/verdict.prompt")"
  [[ -z "${text//[[:space:]]/}" ]] && text="_(verdict unavailable: the chair did not answer)_"
  printf '## 🪑 Verdict (%s)\n\n%s\n' "$title" "$text" > "$WORK/verdict.md"
}

body_of_file() {
  local f="$1" b
  [[ -f "$f" ]] || { echo "_(no output)_"; return; }
  b="$(cat "$f")"
  case "$b" in
    __MISSING__*) echo "_(unavailable — skipped)_";;
    __TIMEOUT__*) echo "_(timed out)_";;
    __EMPTY__*)   echo "_(no answer)_";;
    *) echo "$b";;
  esac
}

OUT="$WORK/transcript.md"
HEAD_MD="$WORK/head.md"; BODY_MD="$WORK/body.md"
: > "$HEAD_MD"; : > "$BODY_MD"
say() { printf '%s\n' "$*" >>"$BODY_MD"; }
# seat_body <i> <file>: the answer, with the lab label first on a lab seat.
seat_body() {
  if [[ -n "${SEAT_DISC[$1]}" ]]; then printf '_%s_\n\n' "${SEAT_DISC[$1]}"; fi
  body_of_file "$2"
}

{ echo "# 🪑 Round Table — $NSEATS $([[ $PERSONA_SEATS -eq 1 ]] && echo seats || echo heads) — $([[ $ROUNDS -le 1 ]] && echo 'advisory' || echo "deliberation (≤$ROUNDS rounds)")"
  echo ""
  echo "_Question:_ ${QUESTION}"
  echo ""
  if [[ -n "$BRIEF" ]]; then echo "## Brief"; echo ""; echo "$BRIEF"; echo ""; fi
} > "$HEAD_MD"

ROUNDS_RUN=1
if [[ "$ROUNDS" -le 1 ]]; then
  printf '%s' "$BASE_PROMPT" > "$PFILE"
  first_round
  for ((i=0; i<NSEATS; i++)); do say "---"; say "## $(seat_label "$i")"; say ""; say "$(seat_body "$i" "$WORK/s$i/${SEAT_HEAD[$i]}.out")"; say ""; done
  [[ -n "$VERDICT" ]] && verdict_text "/"
  FOOTER="_Advisory mode — independent answers, no cross-talk. Use \`--rounds N\` for deliberation-to-consensus._"
else
  CONS=0
  : > "$WORK/chair.last"
  for ((r=1; r<=ROUNDS; r++)); do
    ROUNDS_RUN=$r
    if [[ $r -eq 1 ]]; then
      printf '%s' "$BASE_PROMPT" > "$PFILE"
    else
      printf '%s' "$BASE_PROMPT" > "$WORK/base.md"
      positions ".r$((r-1)).out" > "$WORK/prev.pos"
      python3 "$COACH_PY" deliberation-prompt "$WORK/base.md" "$((r-1))" "$WORK/prev.pos" "$r" "$WORK/chair.last" > "$PFILE"
    fi
    if [[ $r -eq 1 ]]; then first_round; else run_all_seats; fi
    LAST_R=$r
    for ((i=0; i<NSEATS; i++)); do cp "$WORK/s$i/${SEAT_HEAD[$i]}.out" "$WORK/s$i.r$r.out" 2>/dev/null; done
    say "## ─────── Round $r ───────"; say ""
    for ((i=0; i<NSEATS; i++)); do say "### $(seat_label "$i")"; say ""; say "$(seat_body "$i" "$WORK/s$i.r$r.out")"; say ""; done
    positions ".r$r.out" > "$WORK/round.pos"
    python3 "$COACH_PY" chair-prompt "$QUESTION" "$r" "$WORK/round.pos" > "$WORK/chair.prompt"
    CHAIR_OUT="$(anthropic_call "$CHAIR_MODEL" "$WORK/chair.prompt")"
    printf '%s' "$CHAIR_OUT" > "$WORK/chair.last"
    [[ -z "$CHAIR_OUT" ]] && CHAIR_OUT="VERDICT: CONTINUE"$'\n'"_(chair unavailable — ANTHROPIC_API_KEY missing/error)_"
    say "### 🪑 Chair verdict — round $r"; say ""; say "$CHAIR_OUT"; say ""
    # Early stop only on true agreement: the same pick, numbers, thresholds and first step.
    if printf '%s' "$CHAIR_OUT" | python3 "$COACH_PY" agreed; then
      CONS=1; say "**→ Consensus reached at round $r.**"; say ""; break
    fi
  done
  [[ -n "$VERDICT" ]] && verdict_text ".r$LAST_R.out"
  if [[ $CONS -eq 1 ]]; then FOOTER="_Deliberation converged. The chair's final verdict above is the council consensus._"
  else FOOTER="_Reached the round cap ($ROUNDS) without full consensus — the chair's last verdict lists the remaining disagreements._"; fi
fi

# The verdict first; the seat answers folded under it.
if [[ -n "$VERDICT" ]]; then
  if [[ "$NSEATS" -eq 1 ]]; then _seats="1 seat"; else _seats="$NSEATS seats"; fi
  if [[ "$ROUNDS_RUN" -le 1 ]]; then _sum="<details><summary>The seat answers ($_seats)</summary>"
  else _sum="<details><summary>The seat answers and the rounds ($_seats, $ROUNDS_RUN rounds)</summary>"; fi
  { cat "$HEAD_MD"; echo "---"; cat "$WORK/verdict.md"; echo ""; echo "---"; echo "$_sum"; echo ""; cat "$BODY_MD"; echo "</details>"; echo ""; echo "---"; echo "$FOOTER"; } > "$OUT"
else
  { cat "$HEAD_MD"; cat "$BODY_MD"; echo "---"; echo "$FOOTER"; } > "$OUT"
fi

[[ -n "$OUT_FILE" ]] && cp "$OUT" "$OUT_FILE"
cat "$OUT"
