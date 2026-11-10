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
#   core.sh -q "..." --verdict decision              # final verdict: decision|pros-cons|plan|vote (+ minority view)
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
    --verdict)     VERDICT="$2"; shift 2 ;;
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
  # Fallback: grok CLI (coding-plan composer).
  command -v grok >/dev/null 2>&1 || { echo "__MISSING__ (no XAI_API_KEY / OAuth token / grok CLI)" >"$out"; return; }
  local p; p="$(cat "$PFILE")"
  local gm=(); [[ -n "$GROK_CLI_MODEL" ]] && gm=(-m "$GROK_CLI_MODEL")
  if [[ "$RESEARCH" -eq 1 ]]; then run_to "$TIMEOUT" grok -p "$p" ${gm[@]+"${gm[@]}"} --max-turns 8 --no-subagents </dev/null >"$out" 2>"$err"
  else run_to "$TIMEOUT" grok -p "$p" ${gm[@]+"${gm[@]}"} --max-turns 6 --no-subagents --disable-web-search </dev/null >"$out" 2>"$err"; fi
  local rc=$?
  [[ $rc -eq 124 ]] && echo "__TIMEOUT__ (${TIMEOUT}s)" >"$out"
  [[ ! -s "$out" ]] && echo "__EMPTY__ (rc=$rc) $(tail -c 200 "$err" 2>/dev/null)" >"$out"
}

run_codex() {
  local out="$WORK/codex.out" err="$WORK/codex.err" last="$WORK/codex.last"
  command -v codex >/dev/null 2>&1 || { echo "__MISSING__" >"$out"; return; }
  local p; p="$(cat "$PFILE")"
  if [[ "$RESEARCH" -eq 1 ]]; then run_to "$TIMEOUT" codex exec --skip-git-repo-check -s read-only -o "$last" "$p" >"$err" 2>&1
  else run_to "$TIMEOUT" codex exec --skip-git-repo-check -s read-only -c tools.web_search=false -o "$last" "$p" >"$err" 2>&1; fi
  local rc=$?
  [[ $rc -eq 124 ]] && { echo "__TIMEOUT__ (${TIMEOUT}s)" >"$out"; return; }
  if [[ -s "$last" ]]; then cp "$last" "$out"; else echo "__EMPTY__ (rc=$rc) $(tail -c 200 "$err" 2>/dev/null)" >"$out"; fi
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
if [[ -n "$BASE_PROMPT" ]]; then BASE_PROMPT+=$'\n\n'"## QUESTION"$'\n'"$QUESTION"; else BASE_PROMPT="$QUESTION"; fi

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

# say_verdict <file suffix>: the chair's final verdict in the --verdict shape, with the minority view.
say_verdict() {
  [[ -z "$VERDICT" ]] && return
  local i f job title text
  case "$VERDICT" in
    decision)  title="decision"; job='Shape: DECISION. Line 1: "DECISION: <the answer in one sentence>". Line 2: "CONFIDENCE: <high, medium or low> (<0 to 100>%)". Then **WHY** (at most four bullets).' ;;
    pros-cons) title="pros and cons"; job='Shape: PROS AND CONS. Line 1: "OPTION: <the leading option>". Then **PROS** (bullets) and **CONS** (bullets), then "RECOMMENDATION: <one sentence>".' ;;
    plan)      title="action plan"; job='Shape: ACTION PLAN. Line 1: "GOAL: <one sentence>". Then **STEPS** as a numbered list, each step with who and by when, then "FIRST STEP TODAY: <one sentence>".' ;;
    vote)      title="vote"; job='Shape: VOTE. Give **VOTES**: one bullet per seat, "<seat name>: <the option it backs>". Then "RESULT: <option> wins <n> to <m>", or "RESULT: tie".' ;;
  esac
  { printf 'You are the neutral CHAIR of an AI council. Write the final verdict on this question:\n\n%s\n\n' "$QUESTION"
    [[ -n "$BRIEF" ]] && printf 'THE BRIEF:\n%s\n\n' "$BRIEF"
    printf -- '--- FINAL POSITIONS ---\n\n'
    for ((i=0; i<NSEATS; i++)); do
      f="$WORK/s$i$1"; [[ "$1" == "/" ]] && f="$WORK/s$i/${SEAT_HEAD[$i]}.out"
      printf '### %s\n%s\n\n' "$(seat_label "$i")" "$(body_of_file "$f")"
    done
    printf -- '--- YOUR JOB ---\n%s\nThen end with exactly these two lines:\nMINORITY VIEW: <the strongest view that differs from the verdict, in one sentence, or "None">\nHELD BY: <the seats that held it, by their names above, or "Nobody">\n' "$job"
  } > "$WORK/verdict.prompt"
  text="$(anthropic_call "$CHAIR_MODEL" "$WORK/verdict.prompt")"
  [[ -z "${text//[[:space:]]/}" ]] && text="_(verdict unavailable: the chair did not answer)_"
  say "---"; say "## 🪑 Verdict ($title)"; say ""; say "$text"; say ""
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
say() { printf '%s\n' "$*" >>"$OUT"; }
# seat_body <i> <file>: the answer, with the lab label first on a lab seat.
seat_body() {
  if [[ -n "${SEAT_DISC[$1]}" ]]; then printf '_%s_\n\n' "${SEAT_DISC[$1]}"; fi
  body_of_file "$2"
}

say "# 🪑 Round Table — $NSEATS $([[ $PERSONA_SEATS -eq 1 ]] && echo seats || echo heads) — $([[ $ROUNDS -le 1 ]] && echo 'advisory' || echo "deliberation (≤$ROUNDS rounds)")"
say ""
say "_Question:_ ${QUESTION}"
say ""
if [[ -n "$BRIEF" ]]; then say "## Brief"; say ""; say "$BRIEF"; say ""; fi

if [[ "$ROUNDS" -le 1 ]]; then
  printf '%s' "$BASE_PROMPT" > "$PFILE"
  first_round
  for ((i=0; i<NSEATS; i++)); do say "---"; say "## $(seat_label "$i")"; say ""; say "$(seat_body "$i" "$WORK/s$i/${SEAT_HEAD[$i]}.out")"; say ""; done
  say_verdict "/"
  say "---"
  say "_Advisory mode — independent answers, no cross-talk. Use \`--rounds N\` for deliberation-to-consensus._"
else
  CONS=0
  for ((r=1; r<=ROUNDS; r++)); do
    if [[ $r -eq 1 ]]; then
      printf '%s' "$BASE_PROMPT" > "$PFILE"
    else
      { printf '%s\n\n---\n## COUNCIL DELIBERATION — every councilor'\''s position from round %d:\n\n' "$BASE_PROMPT" "$((r-1))"
        for ((i=0; i<NSEATS; i++)); do printf '### %s\n%s\n\n' "$(seat_label "$i")" "$(body_of_file "$WORK/s$i.r$((r-1)).out")"; done
        printf '## YOUR TASK (round %d)\nYou are ONE councilor. You have now read everyone. Update your position. State explicitly: (a) where a peer changed your mind — name them; (b) where you still disagree and why; (c) your sharpened current answer. Begin your reply with "POSITION:".\n' "$r"
      } > "$PFILE"
    fi
    if [[ $r -eq 1 ]]; then first_round; else run_all_seats; fi
    LAST_R=$r
    for ((i=0; i<NSEATS; i++)); do cp "$WORK/s$i/${SEAT_HEAD[$i]}.out" "$WORK/s$i.r$r.out" 2>/dev/null; done
    say "## ─────── Round $r ───────"; say ""
    for ((i=0; i<NSEATS; i++)); do say "### $(seat_label "$i")"; say ""; say "$(seat_body "$i" "$WORK/s$i.r$r.out")"; say ""; done
    { printf 'You are the neutral CHAIR of an AI design council. The councilors submitted these round %d positions to this question:\n\n%s\n\n--- POSITIONS ---\n\n' "$r" "$QUESTION"
      for ((i=0; i<NSEATS; i++)); do printf '### %s\n%s\n\n' "$(seat_label "$i")" "$(body_of_file "$WORK/s$i.r$r.out")"; done
      printf -- '--- YOUR JOB ---\nLine 1 must be EXACTLY one of: "VERDICT: CONSENSUS" (the councilors substantively agree on the core recommendation) or "VERDICT: CONTINUE" (material disagreement remains). Then give: **AGREED** (bullets) and **REMAINING DISAGREEMENTS** (bullets — name who holds what). Be strict: consensus means real convergence on the answer, not mere overlap.\n'
    } > "$WORK/chair.prompt"
    CHAIR_OUT="$(anthropic_call "$CHAIR_MODEL" "$WORK/chair.prompt")"
    [[ -z "$CHAIR_OUT" ]] && CHAIR_OUT="VERDICT: CONTINUE"$'\n'"_(chair unavailable — ANTHROPIC_API_KEY missing/error)_"
    say "### 🪑 Chair verdict — round $r"; say ""; say "$CHAIR_OUT"; say ""
    if printf '%s' "$CHAIR_OUT" | head -1 | grep -qi "VERDICT: *CONSENSUS"; then
      CONS=1; say "**→ Consensus reached at round $r.**"; say ""; break
    fi
  done
  say_verdict ".r$LAST_R.out"
  say "---"
  if [[ $CONS -eq 1 ]]; then say "_Deliberation converged. The chair's final verdict above is the council consensus._"
  else say "_Reached the round cap ($ROUNDS) without full consensus — the chair's last verdict lists the remaining disagreements._"; fi
fi

[[ -n "$OUT_FILE" ]] && cp "$OUT" "$OUT_FILE"
cat "$OUT"
