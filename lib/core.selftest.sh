#!/usr/bin/env bash
# core.selftest.sh — offline, assert-based self-check for lib/core.sh. NO network.
#
# Proves the genericized engine runs for ANY user with just API keys (or none):
#   1. `core.sh -h` exits 0 and prints usage.
#   2. With an empty $HOME and no keys, `--heads claude` exits 0 and the table
#      prints a graceful skip sentinel for claude (no crash, no network).
#   3. `--heads bogus` warns about the unknown head and does not crash.
#   4. (bonus) `~/.config/roundtable/config.env` is sourced — an ROUNDTABLE_*_MODEL
#      override there shows up in the printed header (offline proof of sourcing).
#   5. `--verdict FILE` writes a structured verdict record for an advisory run.
#   6. `--rounds 2 --verdict FILE` records an unreachable chair as CONTINUE.
#   7. `--sign` with no key configured still lands the unsigned record (fail-open).
#
# The chair/head parsing itself is covered separately by lib/verdict.selftest.sh.
#
# Run:  bash lib/core.selftest.sh
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CORE="$HERE/core.sh"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); printf '  \xe2\x9c\x85 %s\n' "$1"; }
bad() { FAIL=$((FAIL+1)); printf '  \xe2\x9d\x8c %s\n' "$1"; }
snip() { printf '%s' "$1" | tr '\n' ' ' | cut -c1-200; }

# Run core.sh in a hermetic env: only HOME + PATH (+ TMPDIR) survive, so no inherited
# API key can leak in and the heads are forced down their graceful-skip paths.
SAFE_PATH="/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin"
run_core() { local home="$1"; shift; env -i HOME="$home" PATH="$SAFE_PATH" TMPDIR="${TMPDIR:-/tmp}" bash "$CORE" "$@"; }

[[ -f "$CORE" ]] || { echo "FATAL: core.sh not found at $CORE" >&2; exit 1; }
echo "roundtable core.sh self-test (offline)"
echo "core: $CORE"
echo

# ── Test 1: -h exits 0 and prints usage ──────────────────────────────────────
echo "[1] core.sh -h"
T1="$(mktemp -d)"
out="$(run_core "$T1" -h 2>&1)"; rc=$?
if [[ $rc -eq 0 ]]; then
  if printf '%s' "$out" | grep -q "USAGE"; then ok "-h exits 0 and prints usage"; else bad "-h exit 0 but no USAGE block"; fi
else
  bad "-h exit code was $rc (expected 0)"
fi
rm -rf "$T1"

# ── Test 2: no keys, --heads claude → graceful skip + table, exit 0 ───────────
echo "[2] empty HOME, no keys, --heads claude"
T2="$(mktemp -d)"   # empty: no ~/.config/roundtable/config.env, no ~/.api_keys
out="$(run_core "$T2" -q "x" --heads claude 2>&1)"; rc=$?
if [[ $rc -eq 0 ]]; then
  if printf '%s' "$out" | grep -q "Round Table" && printf '%s' "$out" | grep -Eq "no answer|unavailable"; then
    ok "no-key claude run exits 0; table prints with a skip sentinel"
  else
    bad "missing table or skip sentinel; out=[$(snip "$out")]"
  fi
else
  bad "no-key claude run exit code was $rc (expected 0); out=[$(snip "$out")]"
fi
rm -rf "$T2"

# ── Test 3: --heads bogus → warns, does not crash ────────────────────────────
echo "[3] --heads bogus"
T3="$(mktemp -d)"
out="$(run_core "$T3" -q "x" --heads bogus 2>&1)"; rc=$?
if printf '%s' "$out" | grep -q "unknown head 'bogus'"; then
  # A set -u empty-array abort would be rc=1; our explicit guard exits 2. Either way: warned, no crash.
  if [[ $rc -ne 1 ]]; then ok "unknown head warns and does not crash (rc=$rc)"; else bad "unknown head crashed with rc=1 (set -u abort?)"; fi
else
  bad "no warning for unknown head; out=[$(snip "$out")]"
fi
rm -rf "$T3"

# ── Test 4: config.env is sourced (offline model-override proof) ─────────────
echo "[4] config.env sourced (ROUNDTABLE_CLAUDE_MODEL override)"
T4="$(mktemp -d)"
mkdir -p "$T4/.config/roundtable"
printf 'export ROUNDTABLE_CLAUDE_MODEL=selftest-model-xyz\n' > "$T4/.config/roundtable/config.env"
out="$(run_core "$T4" -q "x" --heads claude 2>&1)"; rc=$?
if [[ $rc -eq 0 ]] && printf '%s' "$out" | grep -q "selftest-model-xyz"; then
  ok "config.env sourced — override appears in the Claude header"
else
  bad "config.env override not honored (rc=$rc); out=[$(snip "$out")]"
fi
rm -rf "$T4"

# ── verdict artifact wiring (offline; no keys, so no head or chair ever answers) ──
jget() {
  python3 - "$1" "$2" <<'PY'
import json, sys
node = json.load(open(sys.argv[1]))
for key in sys.argv[2].split("."):
    node = node[int(key)] if key.isdigit() else (node or {}).get(key)
sys.stdout.write("" if node is None else str(node))
PY
}

# ── Test 5: --verdict emits a structured record for an advisory run ──────────
echo "[5] --verdict writes a structured record (advisory)"
T5="$(mktemp -d)"
out="$(run_core "$T5" -q "does the record land" --heads claude --verdict "$T5/v.json" 2>&1)"; rc=$?
if [[ $rc -eq 0 && -f "$T5/v.json" ]]; then
  v="$(jget "$T5/v.json" verdict)"; qt="$(jget "$T5/v.json" question.text)"
  st="$(jget "$T5/v.json" heads.0.status)"
  if [[ "$v" = "NO_CHAIR" && "$qt" = "does the record land" && "$st" = "empty" ]]; then
    ok "advisory run emits verdict.json (verdict=$v, head=$st)"
  else
    bad "unexpected record: verdict=[$v] question=[$qt] head=[$st]"
  fi
  printf '%s' "$out" | grep -q "Verdict record" && ok "transcript points at the record" \
                                                || bad "transcript does not mention the record"
else
  bad "--verdict run rc=$rc, record present: $([[ -f "$T5/v.json" ]] && echo yes || echo no); out=[$(snip "$out")]"
fi
rm -rf "$T5"

# ── Test 6: unreachable chair is recorded honestly, not as consensus ─────────
echo "[6] --rounds 2 --verdict with no chair key"
T6="$(mktemp -d)"
out="$(run_core "$T6" -q "x" --heads claude --rounds 2 --verdict "$T6/v.json" 2>&1)"; rc=$?
if [[ $rc -eq 0 && -f "$T6/v.json" ]]; then
  v="$(jget "$T6/v.json" verdict)"; cs="$(jget "$T6/v.json" chair.status)"; rr="$(jget "$T6/v.json" run.rounds_run)"
  if [[ "$v" = "CONTINUE" && "$cs" = "unreachable" && "$rr" = "2" ]]; then
    ok "keyless chair recorded as unreachable/CONTINUE after $rr rounds"
  else
    bad "unexpected record: verdict=[$v] chair.status=[$cs] rounds_run=[$rr]"
  fi
else
  bad "deliberation --verdict run rc=$rc; out=[$(snip "$out")]"
fi
rm -rf "$T6"

# ── Test 7: --sign is fail-open — the unsigned record still lands ────────────
echo "[7] --sign with no signing key configured"
T7="$(mktemp -d)"
out="$(run_core "$T7" -q "x" --heads claude --verdict "$T7/v.json" --sign 2>&1)"; rc=$?
if [[ $rc -eq 0 && -f "$T7/v.json" ]] && [[ ! -f "$T7/v.aar.json" ]]; then
  ok "unsigned verdict record still lands; no AAR written; run exit 0"
else
  bad "fail-open broke: rc=$rc record=$([[ -f "$T7/v.json" ]] && echo yes || echo no) aar=$([[ -f "$T7/v.aar.json" ]] && echo yes || echo no)"
fi
[[ -f "$T7/v.transcript.md" ]] && ok "--sign retains the transcript (AAR evidence custody)" \
                               || bad "--sign did not retain a transcript for evidence custody"
rm -rf "$T7"

echo
echo "── result: $PASS passed, $FAIL failed ──"
[[ $FAIL -eq 0 ]]
