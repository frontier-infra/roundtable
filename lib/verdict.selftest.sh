#!/usr/bin/env bash
# verdict.selftest.sh — offline, assert-based self-check for lib/verdict.py. NO network,
# NO model calls: every council run below is a canned transcript written into a temp dir.
#
#   1. deliberation run → verdict.json with the right verdict, head statuses, hashes, bullets
#   2. chair line missing            → CONTINUE + chair.status=unparsed + a warning, record still lands
#   3. malformed bullets             → prose captured, warned, record still valid JSON
#   4. chair unreachable             → chair.status=unreachable, verdict CONTINUE
#   5. advisory run (no chair)       → verdict NO_CHAIR
#   6. engine decision vs local parse→ the engine's decision wins, disagreement warned
#   7. signing tool absent / unconfigured → exit 3, NO .aar.json, verdict.json untouched
#   8. real signer present (opt-in)  → keygen → sign → verify round-trip, offline
#
# Test 8 needs a checkout of the AAR reference signer. Point at it to enable:
#   ROUNDTABLE_AAR_TOOL=/path/to/agentcontrolplane/tools/aar.mjs bash lib/verdict.selftest.sh
# Without it that one case reports SKIP (never a silent pass).
#
# Run:  bash lib/verdict.selftest.sh
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VP="$HERE/verdict.py"
PASS=0; FAIL=0; SKIP=0
ok()   { PASS=$((PASS+1)); printf '  \xe2\x9c\x85 %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf '  \xe2\x9d\x8c %s\n' "$1"; }
skip() { SKIP=$((SKIP+1)); printf '  \xe2\x9e\x96 SKIP %s\n' "$1"; }
eq()   { if [ "$2" = "$3" ]; then ok "$1 ($2)"; else bad "$1: expected [$3], got [$2]"; fi; }

[[ -f "$VP" ]] || { echo "FATAL: verdict.py not found at $VP" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 is required" >&2; exit 1; }

ROOT="$(mktemp -d "${TMPDIR:-/tmp}/rt-verdict-test.XXXXXX")"
trap 'rm -rf "$ROOT"' EXIT
# Hermetic signing config: never read the developer's real ~/.config/roundtable.
SANDBOX="$ROOT/xdg"; mkdir -p "$SANDBOX/roundtable"

# jget FILE dotted.path  → scalar as text, containers as JSON
jget() {
  python3 - "$1" "$2" <<'PY'
import json, sys
node = json.load(open(sys.argv[1]))
for key in sys.argv[2].split("."):
    node = node[int(key)] if key.lstrip("-").isdigit() else (node or {}).get(key)
if node is None: sys.stdout.write("")
elif isinstance(node, (dict, list)): sys.stdout.write(json.dumps(node, ensure_ascii=False))
else: sys.stdout.write(str(node))
PY
}
sha_of() { python3 -c 'import hashlib,sys;sys.stdout.write(hashlib.sha256(sys.argv[1].strip().encode()).hexdigest())' "$1"; }
emit() { python3 "$VP" emit --quiet "$@"; }
# verdict.py sign, with the developer's own config walled off.
sign_isolated() { env XDG_CONFIG_HOME="$SANDBOX" "$@"; }

echo "roundtable verdict.py self-test (offline)"
echo "verdict.py: $VP"
echo

# ── shared fixture: a 2-round deliberation that converges ────────────────────
QTEXT='Should the ratifier live in Roundtable or in The Machine?'
GROK_R2='POSITION: Roundtable should emit the record; The Machine should consume it.'
CLAUDE_R2='POSITION: Agreed — producing and consuming the ratification are different jobs.'

R1="$ROOT/run1"; mkdir -p "$R1"
printf '%s' "$QTEXT" > "$R1/question.txt"
printf 'grok\t🤖 Grok (grok-4.3, xAI)\tgrok-4.3\n'          > "$R1/heads.tsv"
printf 'claude\t🔵 Claude (claude-opus-4-8)\tclaude-opus-4-8\n' >> "$R1/heads.tsv"
printf 'minimax\t🟠 MiniMax (MiniMax-M3)\tMiniMax-M3\n'      >> "$R1/heads.tsv"
printf 'gemini\t✨ Gemini (gemini-3.1-pro-preview)\tgemini-3.1-pro-preview\n' >> "$R1/heads.tsv"

printf '%s' 'POSITION: Roundtable is the wrong place; The Machine already gates.' > "$R1/grok.r1.out"
printf '%s' 'POSITION: Roundtable is the organ that produces ratification.'       > "$R1/claude.r1.out"
printf '%s' '__TIMEOUT__ (300s)'                                                  > "$R1/minimax.r1.out"
printf '%s' '__MISSING__ (no GEMINI_API_KEY/GOOGLE_API_KEY)'                      > "$R1/gemini.r1.out"
printf '%s' "$GROK_R2"                                                            > "$R1/grok.r2.out"
printf '%s' "$CLAUDE_R2"                                                          > "$R1/claude.r2.out"
printf '%s' '__TIMEOUT__ (300s)'                                                  > "$R1/minimax.r2.out"
printf '%s' '__MISSING__ (no GEMINI_API_KEY/GOOGLE_API_KEY)'                      > "$R1/gemini.r2.out"

cat > "$R1/chair.r1.out" <<'EOF'
VERDICT: CONTINUE

**AGREED**
- The verdict must be machine-consumable.

**REMAINING DISAGREEMENTS**
- Grok wants The Machine to own it; Claude wants Roundtable to emit it.
EOF
printf 'CONTINUE\n' > "$R1/chair.r1.decision"; printf 'ok\n' > "$R1/chair.r1.status"
cat > "$R1/chair.r2.out" <<'EOF'
VERDICT: CONSENSUS

**AGREED**
- Roundtable emits the signed verdict.
- The Machine consumes it at the Box-0 gate.

**REMAINING DISAGREEMENTS**
- None material.
EOF
printf 'CONSENSUS\n' > "$R1/chair.r2.decision"; printf 'ok\n' > "$R1/chair.r2.status"
printf '2026-08-12T21:00:10Z\n' > "$R1/r1.finished"
printf '2026-08-12T21:00:20Z\n' > "$R1/r2.finished"
printf '2026-08-12T21:00:30Z\n' > "$R1/chair.r2.finished"

echo "[1] deliberation run → structured verdict.json"
V1="$ROOT/verdict1.json"
emit --run-dir "$R1" --out "$V1" --question-file "$R1/question.txt" --heads-file "$R1/heads.tsv" \
     --mode deliberation --rounds-max 3 --rounds-run 2 --chair-model claude-opus-4-8 \
     --started-at 2026-08-12T21:00:00Z --finished-at 2026-08-12T21:00:31Z 2>/dev/null
rc=$?
if [[ $rc -ne 0 || ! -f "$V1" ]]; then
  bad "emit failed (rc=$rc) — remaining assertions skipped"
else
  ok "emit exits 0 and writes $V1"
  eq "verdict"                "$(jget "$V1" verdict)"                  "CONSENSUS"
  eq "question verbatim"      "$(jget "$V1" question.text)"            "$QTEXT"
  eq "question sha256"        "$(jget "$V1" question.sha256)"          "$(sha_of "$QTEXT")"
  eq "heads answered"         "$(jget "$V1" head_counts.answered)"     "2"
  eq "heads timed out"        "$(jget "$V1" head_counts.timeout)"      "1"
  eq "heads missing"          "$(jget "$V1" head_counts.missing)"      "1"
  eq "heads configured"       "$(jget "$V1" head_counts.configured)"   "4"
  eq "grok answer hash"       "$(jget "$V1" heads.0.answer_sha256)"    "$(sha_of "$GROK_R2")"
  eq "claude answer hash"     "$(jget "$V1" heads.1.answer_sha256)"    "$(sha_of "$CLAUDE_R2")"
  eq "timed-out head hashes"  "$(jget "$V1" heads.2.answer_sha256)"    ""
  eq "chair model"            "$(jget "$V1" chair.model)"              "claude-opus-4-8"
  eq "chair status"           "$(jget "$V1" chair.status)"             "ok"
  eq "chair round"            "$(jget "$V1" chair.round)"              "2"
  eq "rounds recorded"        "$(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))["rounds"]))' "$V1")" "2"
  eq "round 1 verdict kept"   "$(jget "$V1" rounds.0.chair.decision)"  "CONTINUE"
  eq "agreed bullets"         "$(jget "$V1" agreed)" '["Roundtable emits the signed verdict.", "The Machine consumes it at the Box-0 gate."]'
  eq "disagreement bullets"   "$(jget "$V1" disagreements)" '["None material."]'
  eq "no chair warnings"      "$(jget "$V1" chair.warnings)"           "[]"
  eq "schema version"         "$(jget "$V1" schema_version)"           "0.1"
fi

echo "[2] chair output with no VERDICT line"
R2="$ROOT/run2"; mkdir -p "$R2"
printf '%s' "$QTEXT" > "$R2/question.txt"; printf 'claude\tClaude\tclaude-opus-4-8\n' > "$R2/heads.tsv"
printf '%s' 'POSITION: emit it.' > "$R2/claude.r1.out"
cat > "$R2/chair.r1.out" <<'EOF'
The council mostly agrees, though I would not call it settled.

**AGREED**
- Something should be emitted.
EOF
V2="$ROOT/verdict2.json"
emit --run-dir "$R2" --out "$V2" --question-file "$R2/question.txt" --heads-file "$R2/heads.tsv" \
     --mode deliberation --rounds-max 1 --rounds-run 1 --chair-model claude-opus-4-8 \
     --finished-at 2026-08-12T21:05:00Z 2>"$ROOT/e2"
rc=$?
[[ $rc -eq 0 && -f "$V2" ]] && ok "record still emitted (rc=0) despite an unparseable chair line" \
                            || bad "emit rc=$rc / missing record on the unparsed-chair path"
if [[ -f "$V2" ]]; then
  eq "verdict falls back conservatively" "$(jget "$V2" verdict)" "CONTINUE"
  eq "chair status"                      "$(jget "$V2" chair.status)" "unparsed"
  if grep -q "VERDICT" "$ROOT/e2" && jget "$V2" chair.warnings | grep -q "no 'VERDICT"; then
    ok "warns (record + stderr) that no VERDICT token was found"
  else
    bad "missing warning; warnings=[$(jget "$V2" chair.warnings)]"
  fi
fi

echo "[3] malformed bullets (prose sections, odd headings)"
R3="$ROOT/run3"; mkdir -p "$R3"
printf '%s' "$QTEXT" > "$R3/question.txt"; printf 'claude\tClaude\tclaude-opus-4-8\n' > "$R3/heads.tsv"
printf '%s' 'POSITION: emit it.' > "$R3/claude.r1.out"
cat > "$R3/chair.r1.out" <<'EOF'
VERDICT: CONTINUE
AGREED:
Everyone accepts that the record must be signed. There was some disagreement
about naming, but nobody objected to the signing itself.
EOF
printf 'CONTINUE\n' > "$R3/chair.r1.decision"; printf 'ok\n' > "$R3/chair.r1.status"
V3="$ROOT/verdict3.json"
emit --run-dir "$R3" --out "$V3" --question-file "$R3/question.txt" --heads-file "$R3/heads.tsv" \
     --mode deliberation --rounds-max 1 --rounds-run 1 --chair-model claude-opus-4-8 \
     --finished-at 2026-08-12T21:06:00Z 2>/dev/null
rc=$?
if [[ $rc -eq 0 ]] && python3 -c 'import json,sys;json.load(open(sys.argv[1]))' "$V3" 2>/dev/null; then
  ok "malformed bullets still produce valid JSON (rc=0)"
  eq "verdict still read"   "$(jget "$V3" verdict)" "CONTINUE"
  if jget "$V3" agreed | grep -q "must be signed"; then ok "prose section captured as one item"
  else bad "prose not captured; agreed=[$(jget "$V3" agreed)]"; fi
  # A prose line mentioning "disagreement" must NOT silently re-section the output.
  eq "prose does not fake a disagreements section" "$(jget "$V3" disagreements)" "[]"
  if jget "$V3" chair.warnings | grep -q "no bullets"; then ok "warns that the section had no bullets"
  else bad "no bullet warning; warnings=[$(jget "$V3" chair.warnings)]"; fi
else
  bad "malformed-bullet run did not produce valid JSON (rc=$rc)"
fi

echo "[4] chair unreachable (core.sh's fallback text + status file)"
R4="$ROOT/run4"; mkdir -p "$R4"
printf '%s' "$QTEXT" > "$R4/question.txt"; printf 'claude\tClaude\tclaude-opus-4-8\n' > "$R4/heads.tsv"
printf '%s' 'POSITION: emit it.' > "$R4/claude.r1.out"
printf 'VERDICT: CONTINUE\n_(chair unavailable — ANTHROPIC_API_KEY missing/error)_' > "$R4/chair.r1.out"
printf 'unreachable\n' > "$R4/chair.r1.status"; printf 'CONTINUE\n' > "$R4/chair.r1.decision"
V4="$ROOT/verdict4.json"
emit --run-dir "$R4" --out "$V4" --question-file "$R4/question.txt" --heads-file "$R4/heads.tsv" \
     --mode deliberation --rounds-max 2 --rounds-run 1 --chair-model claude-opus-4-8 \
     --finished-at 2026-08-12T21:07:00Z 2>/dev/null
eq "chair status" "$(jget "$V4" chair.status)" "unreachable"
eq "verdict"      "$(jget "$V4" verdict)"      "CONTINUE"

echo "[5] advisory run (no chair convened)"
R5="$ROOT/run5"; mkdir -p "$R5"
printf '%s' "$QTEXT" > "$R5/question.txt"
printf 'grok\tGrok\tgrok-4.3\nclaude\tClaude\tclaude-opus-4-8\n' > "$R5/heads.tsv"
printf '%s' 'Postgres.' > "$R5/grok.out"; printf '%s' 'Postgres, yes.' > "$R5/claude.out"
V5="$ROOT/verdict5.json"
emit --run-dir "$R5" --out "$V5" --question-file "$R5/question.txt" --heads-file "$R5/heads.tsv" \
     --mode advisory --rounds-max 1 --rounds-run 1 --finished-at 2026-08-12T21:08:00Z 2>/dev/null
eq "verdict"       "$(jget "$V5" verdict)"              "NO_CHAIR"
eq "chair status"  "$(jget "$V5" chair.status)"         "not_run"
eq "mode"          "$(jget "$V5" run.mode)"             "advisory"
eq "heads answered" "$(jget "$V5" head_counts.answered)" "2"

echo "[6] engine decision wins over a divergent local parse"
R6="$ROOT/run6"; mkdir -p "$R6"
printf '%s' "$QTEXT" > "$R6/question.txt"; printf 'claude\tClaude\tclaude-opus-4-8\n' > "$R6/heads.tsv"
printf '%s' 'POSITION: emit it.' > "$R6/claude.r1.out"
# core.sh greps line 1 for "VERDICT: *CONSENSUS" — this decorated line does NOT match,
# so the engine kept deliberating. The artifact must report what the engine did.
printf '**VERDICT:** CONSENSUS\n\n**AGREED**\n- decorated heading\n' > "$R6/chair.r1.out"
printf 'CONTINUE\n' > "$R6/chair.r1.decision"; printf 'ok\n' > "$R6/chair.r1.status"
V6="$ROOT/verdict6.json"
emit --run-dir "$R6" --out "$V6" --question-file "$R6/question.txt" --heads-file "$R6/heads.tsv" \
     --mode deliberation --rounds-max 2 --rounds-run 1 --chair-model claude-opus-4-8 \
     --finished-at 2026-08-12T21:09:00Z 2>/dev/null
eq "verdict follows the engine" "$(jget "$V6" verdict)" "CONTINUE"
if jget "$V6" chair.warnings | grep -q "authoritative"; then ok "parse/engine divergence is warned, not hidden"
else bad "divergence not warned; warnings=[$(jget "$V6" chair.warnings)]"; fi

echo "[7] signing when it cannot sign (fail-open, record survives)"
V1SUM_BEFORE="$(python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$V1")"
out="$(sign_isolated python3 "$VP" sign "$V1" 2>&1)"; rc=$?
if [[ $rc -eq 3 ]] && printf '%s' "$out" | grep -q "not configured"; then
  ok "unconfigured signing exits 3 with a clear message"
else
  bad "unconfigured signing rc=$rc out=[$(printf '%s' "$out" | tr '\n' ' ')]"
fi
out="$(sign_isolated env ROUNDTABLE_AAR_TOOL="$ROOT/nope/aar.mjs" ROUNDTABLE_AAR_PRIV="$ROOT/nope/k.json" \
        ROUNDTABLE_AAR_PRINCIPAL=did:web:test.example python3 "$VP" sign "$V1" 2>&1)"; rc=$?
if [[ $rc -eq 3 ]] && printf '%s' "$out" | grep -q "not found"; then
  ok "absent signing tool exits 3 with a clear message"
else
  bad "absent-tool signing rc=$rc out=[$(printf '%s' "$out" | tr '\n' ' ')]"
fi
[[ -f "$ROOT/verdict1.aar.json" ]] && bad "an AAR file was written even though signing was impossible" \
                                   || ok "no AAR file written when signing is impossible"
V1SUM_AFTER="$(python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$V1")"
eq "verdict record untouched by failed signing" "$V1SUM_AFTER" "$V1SUM_BEFORE"

echo "[8] real signer round-trip (keygen → sign → verify, offline)"
TOOL="${ROUNDTABLE_AAR_TOOL:-}"
if [[ -z "$TOOL" || ! -f "$TOOL" ]]; then
  skip "no AAR signer — set ROUNDTABLE_AAR_TOOL=/path/to/aar.mjs to run this case"
elif ! command -v node >/dev/null 2>&1; then
  skip "node not on PATH — the AAR signer needs Node >= 20"
else
  K="$ROOT/keys"; mkdir -p "$K"
  if node "$TOOL" keygen --did did:web:test.example --out-priv "$K/priv.json" --out-did "$K/did.json" >/dev/null 2>&1; then
    AARF="$ROOT/verdict1.aar.json"
    out="$(sign_isolated env ROUNDTABLE_AAR_TOOL="$TOOL" ROUNDTABLE_AAR_PRIV="$K/priv.json" \
            ROUNDTABLE_AAR_PRINCIPAL=did:web:test.example python3 "$VP" sign "$V1" --out "$AARF" 2>&1)"; rc=$?
    if [[ $rc -eq 0 && -f "$AARF" ]]; then
      ok "sign exits 0 and writes $AARF"
      eq "AAR verdict mapping (CONSENSUS→verified)" "$(jget "$AARF" verdict)" "verified"
      eq "AAR ground_truth is honest"               "$(jget "$AARF" ground_truth)" "unchecked"
      eq "task.claim is the question"               "$(jget "$AARF" task.claim)" "$QTEXT"
      eq "subject defaults under the principal"     "$(jget "$AARF" subject)" "did:web:test.example:roundtable"
      eq "verifier is structurally separate"        "$(jget "$AARF" verifier.id)" "did:web:test.example:roundtable:chair"
      eq "signature algorithm"                      "$(jget "$AARF" sig.alg)" "Ed25519"
      nchecks="$(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))["checks"]))' "$AARF")"
      eq "checks: record + 2 heads + chair"         "$nchecks" "4"
      if node "$TOOL" verify "$AARF" --did-json "$K/did.json" >"$ROOT/verify.out" 2>&1; then
        ok "reference verifier accepts the record ($(grep -o 'conformance: .*' "$ROOT/verify.out" | tail -1))"
      else
        bad "reference verifier rejected it: $(tr '\n' ' ' < "$ROOT/verify.out")"
      fi
      # Tamper check: flipping a byte must break the signature.
      python3 - "$AARF" "$ROOT/tampered.aar.json" <<'PY'
import json, sys
rec = json.load(open(sys.argv[1])); rec["verdict"] = "not_verified"
json.dump(rec, open(sys.argv[2], "w"), indent=2)
PY
      if node "$TOOL" verify "$ROOT/tampered.aar.json" --did-json "$K/did.json" >/dev/null 2>&1; then
        bad "a tampered record still verified"
      else
        ok "tampered record fails verification"
      fi
    else
      bad "sign rc=$rc out=[$(printf '%s' "$out" | tr '\n' ' ')]"
    fi
  else
    bad "aar.mjs keygen failed — cannot run the round-trip"
  fi
fi

echo
echo "── result: $PASS passed, $FAIL failed, $SKIP skipped ──"
[[ $FAIL -eq 0 ]]
