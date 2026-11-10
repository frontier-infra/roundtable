#!/usr/bin/env bash
# Build the wheel and sdist, check them, install each in a fresh venv, and run
# the command. Needs python3 (3.9+) and network access to PyPI for build/twine.
# Usage: tests/test-build.sh [python]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PY="${1:-python3}"
EXPECT_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/pyproject.toml")"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/rt-build.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

step() { printf '\n== %s\n' "$1"; }

step "build tools"
"$PY" -m venv "$WORK/tools"
"$WORK/tools/bin/python" -m pip install -q --upgrade pip build twine

step "python -m build"
"$WORK/tools/bin/python" -m build --outdir "$WORK/dist" "$ROOT"
ls -l "$WORK/dist"
WHEEL="$(ls "$WORK"/dist/roundtable_sh-"$EXPECT_VERSION"-py3-none-any.whl)"
SDIST="$(ls "$WORK"/dist/roundtable_sh-"$EXPECT_VERSION".tar.gz)"

step "wheel contents"
for f in roundtable_cli/_engine/bin/roundtable roundtable_cli/_engine/lib/core.sh \
         roundtable_cli/_engine/lib/mcp.py roundtable_cli/_engine/lib/personas.py \
         roundtable_cli/_engine/personas/business-roles.json roundtable_cli/_engine/skill/SKILL.md; do
  "$WORK/tools/bin/python" -c "import sys,zipfile; n=zipfile.ZipFile(sys.argv[1]).namelist(); sys.exit(0 if sys.argv[2] in n else 1)" "$WHEEL" "$f" \
    || { echo "missing from wheel: $f"; exit 1; }
  echo "ok $f"
done

step "twine check"
"$WORK/tools/bin/twine" check "$WHEEL" "$SDIST"

for kind in wheel sdist; do
  if [ "$kind" = wheel ]; then ART="$WHEEL"; else ART="$SDIST"; fi
  step "fresh venv: $kind"
  "$PY" -m venv "$WORK/v-$kind"
  "$WORK/v-$kind/bin/python" -m pip install -q "$ART"
  RT="$WORK/v-$kind/bin/roundtable"
  HOME_DIR="$WORK/home-$kind"; mkdir -p "$HOME_DIR"
  run() { env -i HOME="$HOME_DIR" PATH="$WORK/v-$kind/bin:/usr/bin:/bin" "$RT" "$@"; }

  got="$(run --version)"; echo "$got"
  [ "$got" = "roundtable $EXPECT_VERSION" ] || { echo "bad version: $got"; exit 1; }

  # The README's first example. No keys here, so every head is skipped,
  # but the command must run and print the council transcript.
  out="$(run "Reply with exactly: OK three words" --heads claude,gemini 2>&1 || true)"
  echo "$out" | head -3
  echo "$out" | grep -q "Round Table" || { echo "example did not run"; exit 1; }

  # MCP server answers the handshake.
  mcp_out="$(printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    | run mcp serve)"
  echo "$mcp_out" | grep -q '"name": *"roundtable"' || { echo "mcp tools/list failed"; exit 1; }
  echo "mcp tools/list ok"
done

printf '\nPASS: roundtable-sh %s builds, checks, installs and runs.\n' "$EXPECT_VERSION"
