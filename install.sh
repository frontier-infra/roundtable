#!/usr/bin/env bash
# ============================================================================
# Roundtable installer  ·  a Frontier Infra project
# ----------------------------------------------------------------------------
# A council of frontier models — for the decisions that matter.
#
#   curl -fsSL https://roundtable.sh/install.sh | bash
#
# Installs the `roundtable` CLI globally:
#   • engine (bin/ + lib/)  →  ${XDG_DATA_HOME:-~/.local/share}/roundtable
#   • command symlink       →  ~/.local/bin/roundtable
#
# Re-running upgrades in place (idempotent). Your keys/config in
# ~/.config/roundtable/config.env are never touched.
#
# Options:
#   --from <path>   Install from a local checkout instead of fetching a release
#                   (also via the ROUNDTABLE_SRC env var). Used for testing.
#   --yes           Non-interactive: skip the auth/install prompts.
#   -h, --help      Show this help.
# ============================================================================
set -euo pipefail

# ---- release constants (flip these at release) ----------------------------
REPO_SLUG="frontier-infra/roundtable"
# PLACEHOLDER tarball — point at a tagged release at cut, e.g.
#   https://github.com/frontier-infra/roundtable/archive/refs/tags/v0.1.0.tar.gz
RELEASE_TARBALL_URL="https://github.com/${REPO_SLUG}/archive/refs/heads/main.tar.gz"

# ---- install locations -----------------------------------------------------
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
SHARE_DIR="$DATA_HOME/roundtable"
BIN_DIR="$HOME/.local/bin"
SYMLINK="$BIN_DIR/roundtable"

# ---- options ---------------------------------------------------------------
SRC="${ROUNDTABLE_SRC:-}"
ASSUME_YES=false

# ---- colors / logging ------------------------------------------------------
if [ -t 1 ]; then
  R='\033[0;31m'; G='\033[0;32m'; Y='\033[0;33m'; C='\033[0;36m'; B='\033[1m'; N='\033[0m'
else
  R=''; G=''; Y=''; C=''; B=''; N=''
fi
info()  { printf "${C}→${N} %s\n" "$1"; }
ok()    { printf "${G}✓${N} %s\n" "$1"; }
warn()  { printf "${Y}⚠${N} %s\n" "$1" >&2; }
err()   { printf "${R}✗${N} %s\n" "$1" >&2; }

usage() { sed -n '3,20p' "$0" | sed 's/^# \{0,1\}//'; }

# ---- arg parse -------------------------------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    --from)   SRC="${2:-}"; shift 2 ;;
    --from=*) SRC="${1#*=}"; shift ;;
    --yes|-y) ASSUME_YES=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) err "unknown option: $1"; usage; exit 2 ;;
  esac
done

# ---- dep checks ------------------------------------------------------------
need() { command -v "$1" >/dev/null 2>&1; }
require_deps() {
  local missing=""
  need bash    || missing="$missing bash"
  need python3 || missing="$missing python3"
  # curl only required when we have to fetch a release (no --from)
  if [ -z "$SRC" ]; then need curl || missing="$missing curl"; fi
  if [ -n "$missing" ]; then
    err "missing required dependencies:$missing"
    info "install them and re-run. (python3 powers the council engine + MCP server)"
    exit 1
  fi
}

# ---- resolve the source tree (local checkout or fetched tarball) -----------
FETCH_TMP=""
cleanup() { [ -n "$FETCH_TMP" ] && rm -rf "$FETCH_TMP" 2>/dev/null || true; }
trap cleanup EXIT

resolve_source() {
  if [ -n "$SRC" ]; then
    SRC="${SRC%/}"
    [ -d "$SRC" ] || { err "--from path not found: $SRC"; exit 1; }
    info "Installing from local checkout: $SRC"
    return 0
  fi
  info "Fetching Roundtable from $RELEASE_TARBALL_URL"
  FETCH_TMP="$(mktemp -d "${TMPDIR:-/tmp}/roundtable.XXXXXX")"
  if ! curl -fsSL "$RELEASE_TARBALL_URL" -o "$FETCH_TMP/rt.tar.gz"; then
    err "download failed: $RELEASE_TARBALL_URL"
    exit 1
  fi
  tar -xzf "$FETCH_TMP/rt.tar.gz" -C "$FETCH_TMP"
  # GitHub archives extract to <repo>-<ref>/ — grab the first dir.
  SRC="$(find "$FETCH_TMP" -mindepth 1 -maxdepth 1 -type d | head -1)"
  [ -n "$SRC" ] || { err "could not locate extracted source tree"; exit 1; }
}

# ---- place engine ----------------------------------------------------------
install_engine() {
  if [ ! -d "$SRC/bin" ] || [ ! -d "$SRC/lib" ]; then
    err "source tree is missing bin/ or lib/: $SRC"
    exit 1
  fi
  if [ ! -e "$SRC/bin/roundtable" ]; then
    warn "engine dispatcher bin/roundtable not present in source yet —"
    warn "  placing what's available; re-run the installer once it lands."
  fi

  info "Installing engine → $SHARE_DIR"
  mkdir -p "$SHARE_DIR"
  rm -rf "$SHARE_DIR/bin" "$SHARE_DIR/lib" "$SHARE_DIR/skill"
  cp -R "$SRC/bin" "$SHARE_DIR/bin"
  cp -R "$SRC/lib" "$SHARE_DIR/lib"
  [ -e "$SHARE_DIR/bin/roundtable" ] && chmod +x "$SHARE_DIR/bin/roundtable" 2>/dev/null || true

  # Ship the Claude Code skill next to the engine so `roundtable install` can drop
  # it even when the install dir isn't a full repo checkout. lib/install.sh looks
  # for it at <engine>/skill/SKILL.md (REPO_ROOT being the parent of lib/).
  SKILL_IN_SRC="$SRC/plugins/roundtable/skills/roundtable/SKILL.md"
  if [ -f "$SKILL_IN_SRC" ]; then
    mkdir -p "$SHARE_DIR/skill"
    cp "$SKILL_IN_SRC" "$SHARE_DIR/skill/SKILL.md"
  fi

  info "Linking command → $SYMLINK"
  mkdir -p "$BIN_DIR"
  ln -sf "$SHARE_DIR/bin/roundtable" "$SYMLINK"
  ok "roundtable installed"
}

# ---- PATH hint -------------------------------------------------------------
path_hint() {
  case ":$PATH:" in
    *":$BIN_DIR:"*)
      ok "$BIN_DIR is already on your PATH" ;;
    *)
      warn "$BIN_DIR is not on your PATH."
      printf "  Add this line to your shell profile (~/.zshrc or ~/.bashrc):\n\n"
      printf "    ${B}export PATH=\"%s:\$PATH\"${N}\n\n" "$BIN_DIR"
      printf "  Then restart your shell (or run the line above) before using roundtable.\n" ;;
  esac
}

# ---- optional post-install steps ------------------------------------------
ask_yn() { # $1 = prompt ; returns 0 for yes (default No)
  local ans=""
  if [ "$ASSUME_YES" = true ]; then return 1; fi   # --yes skips these optional prompts
  if [ -t 0 ]; then
    read -r -p "$1 [y/N] " ans || ans=""
  elif [ -r /dev/tty ]; then
    printf "%s [y/N] " "$1" > /dev/tty
    IFS= read -r ans < /dev/tty || ans=""
  else
    return 1
  fi
  case "$ans" in [yY]|[yY][eE][sS]) return 0 ;; *) return 1 ;; esac
}

post_install() {
  [ -x "$SYMLINK" ] || return 0
  # Prefer the symlink if it's already on PATH, else call it by full path.
  local rt="$SYMLINK"
  echo ""
  if ask_yn "Configure your API keys now (roundtable auth)?"; then
    "$rt" auth || warn "roundtable auth exited non-zero — run it again later."
  fi
  if ask_yn "Wire Roundtable into your coding harnesses now (roundtable install)?"; then
    "$rt" install || warn "roundtable install exited non-zero — run it again later."
  fi
}

# ---- run -------------------------------------------------------------------
printf "${B}🪑 Roundtable${N} — a council of frontier models · a Frontier Infra project\n\n"
require_deps
resolve_source
install_engine
path_hint
post_install

echo ""
ok "Done. Try:  roundtable \"What should I name this service?\""
info "Next:      roundtable auth   ·   roundtable doctor   ·   roundtable install"
info "Docs:      https://roundtable.sh"
