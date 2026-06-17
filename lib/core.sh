#!/usr/bin/env bash
# core.sh — Roundtable multi-model COUNCIL with optional multi-round DELIBERATION to consensus.
# A Frontier Infra project.
#
# Heads: Grok · Codex (CLI) · OpenAI · GLM · MiniMax · Claude · Gemini — keyed REST APIs (+ optional local CLIs).
# Claude (Anthropic API) also acts as the CHAIR that judges consensus and synthesizes.
#
# MODES:
#   --rounds 1  (default)  ADVISORY     — each head answers once, BLIND to the others. Raw answers.
#   --rounds N  (N>=2)     DELIBERATION — round 1 blind; rounds 2..N each head sees the WHOLE table
#                          and revises; the chair declares CONSENSUS or CONTINUE after each round and
#                          stops early on consensus. Prints every round + the chair's verdicts.
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

while [[ $# -gt 0 ]]; do
  case "$1" in
    -q|--question) QUESTION="$2"; shift 2 ;;
    -c|--context)  CONTEXT_FILE="$2"; shift 2 ;;
    --heads)       HEADS="$2"; shift 2 ;;
    --rounds)      ROUNDS="$2"; shift 2 ;;
    --timeout)     TIMEOUT="$2"; shift 2 ;;
    --research)    RESEARCH=1; shift ;;
    --out)         OUT_FILE="$2"; shift 2 ;;
    -h|--help)     grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)             QUESTION="${QUESTION:+$QUESTION }$1"; shift ;;
  esac
done

[[ -z "$TIMEOUT" ]] && { if [[ "$RESEARCH" -eq 1 ]]; then TIMEOUT=360; else TIMEOUT=300; fi; }
[[ -z "$QUESTION" ]] && { echo "ERROR: no question. Use -q \"...\"." >&2; exit 2; }

BASE_PROMPT="$QUESTION"
if [[ -n "$CONTEXT_FILE" && -f "$CONTEXT_FILE" ]]; then
  BASE_PROMPT="$(printf '## CONTEXT\n%s\n\n## QUESTION\n%s' "$(cat "$CONTEXT_FILE")" "$QUESTION")"
fi

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
GROK_CLI_MODEL="${ROUNDTABLE_GROK_CLI_MODEL:-grok-composer-2.5-fast}" # 🤖 grok CLI fallback (used only if no key/token)
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
  if [[ "$RESEARCH" -eq 1 ]]; then run_to "$TIMEOUT" grok -p "$p" -m "$GROK_CLI_MODEL" --max-turns 8 >"$out" 2>"$err"
  else run_to "$TIMEOUT" grok -p "$p" -m "$GROK_CLI_MODEL" --max-turns 6 --disable-web-search >"$out" 2>"$err"; fi
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
  run_to "$TIMEOUT" curl -sS https://api.anthropic.com/v1/messages \
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

declare -a HARR ACTIVE
IFS=',' read -ra HARR <<< "$HEADS"
ACTIVE=()
for h in "${HARR[@]}"; do
  case "$h" in grok|codex|openai|glm|minimax|claude|gemini) ACTIVE+=("$h");; *) echo "WARN: unknown head '$h'" >&2;; esac
done
# Guard the empty case: bash 3.2 + `set -u` aborts on "${ACTIVE[@]}" when the array is empty,
# so a bad --heads list would otherwise crash. Skip gracefully with a clear message instead.
[[ ${#ACTIVE[@]} -eq 0 ]] && { echo "ERROR: no valid heads selected (after dropping unknown heads)." >&2; exit 2; }

run_all_heads() { local h; for h in "${ACTIVE[@]}"; do "run_$h" & done; wait; }

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

say "# 🪑 Round Table — ${#ACTIVE[@]} heads — $([[ $ROUNDS -le 1 ]] && echo 'advisory' || echo "deliberation (≤$ROUNDS rounds)")"
say ""
say "_Question:_ ${QUESTION}"
say ""

if [[ "$ROUNDS" -le 1 ]]; then
  printf '%s' "$BASE_PROMPT" > "$PFILE"
  run_all_heads
  for h in "${ACTIVE[@]}"; do say "---"; say "## $(label_for "$h")"; say ""; say "$(body_of_file "$WORK/$h.out")"; say ""; done
  say "---"
  say "_Advisory mode — independent answers, no cross-talk. Use \`--rounds N\` for deliberation-to-consensus._"
else
  CONS=0
  for ((r=1; r<=ROUNDS; r++)); do
    if [[ $r -eq 1 ]]; then
      printf '%s' "$BASE_PROMPT" > "$PFILE"
    else
      { printf '%s\n\n---\n## COUNCIL DELIBERATION — every councilor'\''s position from round %d:\n\n' "$BASE_PROMPT" "$((r-1))"
        for h in "${ACTIVE[@]}"; do printf '### %s\n%s\n\n' "$(label_for "$h")" "$(body_of_file "$WORK/$h.r$((r-1)).out")"; done
        printf '## YOUR TASK (round %d)\nYou are ONE councilor. You have now read everyone. Update your position. State explicitly: (a) where a peer changed your mind — name them; (b) where you still disagree and why; (c) your sharpened current answer. Begin your reply with "POSITION:".\n' "$r"
      } > "$PFILE"
    fi
    run_all_heads
    for h in "${ACTIVE[@]}"; do cp "$WORK/$h.out" "$WORK/$h.r$r.out" 2>/dev/null; done
    say "## ─────── Round $r ───────"; say ""
    for h in "${ACTIVE[@]}"; do say "### $(label_for "$h")"; say ""; say "$(body_of_file "$WORK/$h.r$r.out")"; say ""; done
    { printf 'You are the neutral CHAIR of an AI design council. The councilors submitted these round %d positions to this question:\n\n%s\n\n--- POSITIONS ---\n\n' "$r" "$QUESTION"
      for h in "${ACTIVE[@]}"; do printf '### %s\n%s\n\n' "$(label_for "$h")" "$(body_of_file "$WORK/$h.r$r.out")"; done
      printf -- '--- YOUR JOB ---\nLine 1 must be EXACTLY one of: "VERDICT: CONSENSUS" (the councilors substantively agree on the core recommendation) or "VERDICT: CONTINUE" (material disagreement remains). Then give: **AGREED** (bullets) and **REMAINING DISAGREEMENTS** (bullets — name who holds what). Be strict: consensus means real convergence on the answer, not mere overlap.\n'
    } > "$WORK/chair.prompt"
    CHAIR_OUT="$(anthropic_call "$CHAIR_MODEL" "$WORK/chair.prompt")"
    [[ -z "$CHAIR_OUT" ]] && CHAIR_OUT="VERDICT: CONTINUE"$'\n'"_(chair unavailable — ANTHROPIC_API_KEY missing/error)_"
    say "### 🪑 Chair verdict — round $r"; say ""; say "$CHAIR_OUT"; say ""
    if printf '%s' "$CHAIR_OUT" | head -1 | grep -qi "VERDICT: *CONSENSUS"; then
      CONS=1; say "**→ Consensus reached at round $r.**"; say ""; break
    fi
  done
  say "---"
  if [[ $CONS -eq 1 ]]; then say "_Deliberation converged. The chair's final verdict above is the council consensus._"
  else say "_Reached the round cap ($ROUNDS) without full consensus — the chair's last verdict lists the remaining disagreements._"; fi
fi

[[ -n "$OUT_FILE" ]] && cp "$OUT" "$OUT_FILE"
cat "$OUT"
