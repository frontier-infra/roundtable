#!/usr/bin/env bash
# Fail if any doc points people at a broken install.
#   - "roundtable" on PyPI is an unrelated project. Ours is "roundtable-sh".
#   - There is no Homebrew tap yet.
# Usage: scripts/check-install-docs.sh [repo-root]
set -uo pipefail
ROOT="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$ROOT" || exit 2

FILES=$(git ls-files -- '*.md' '*.mdx' '*.html' '*.txt' '*.json' 'install.sh' 'lib/*' 'bin/*' 'roundtable_cli/*' 2>/dev/null \
  | grep -v '^packaging/homebrew/' | grep -v '^scripts/check-install-docs.sh$')
[ -n "$FILES" ] || { echo "check-install-docs: no files found" >&2; exit 2; }

fail=0
# 1) pip / pipx / uv installs of the bare name "roundtable" (anything but roundtable-sh).
BAD_PKG='(pip3?|pipx|uv (tool|pip)) install( +-[-a-zA-Z]+)* +roundtable([^-a-zA-Z0-9_]|$)'
# 2) any Homebrew mention.
BAD_BREW='brew install|homebrew-tap|frontier-infra/tap'

for pat in "$BAD_PKG" "$BAD_BREW"; do
  hits=$(echo "$FILES" | xargs grep -nEi "$pat" 2>/dev/null || true)
  if [ -n "$hits" ]; then
    echo "$hits"
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  echo
  echo "check-install-docs: FAIL. Use roundtable-sh for pip, pipx and uv. Remove brew lines until a tap exists." >&2
  exit 1
fi
echo "check-install-docs: OK ($(echo "$FILES" | wc -l | tr -d ' ') files)"
