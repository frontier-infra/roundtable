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

echo
echo "── result: $PASS passed, $FAIL failed ──"
[[ $FAIL -eq 0 ]]
