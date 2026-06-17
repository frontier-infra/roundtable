#!/usr/bin/env bash
# roundtable/lib/config.sh — auth / models / heads / doctor / config / version / update
#
# Sourced or executed by bin/roundtable. Portable: macOS bash 3.2 + Linux.
# No associative arrays, no config parser/framework, no new deps.
#
# Config home: ${XDG_CONFIG_HOME:-$HOME/.config}/roundtable/config.env  (dotenv, chmod 600)
# The engine (lib/core.sh) sources the same file plus reads ROUNDTABLE_* env overrides.
#
# Keyed heads + env keys:
#   claude=ANTHROPIC_API_KEY  openai=OPENAI_API_KEY  grok=XAI_API_KEY
#   glm=ZAI_API_KEY  minimax=MINIMAX_API_KEY  gemini=GEMINI_API_KEY (or GOOGLE_API_KEY)
#   codex=local `codex` CLI login (no key stored)

# ── self-location (resolve symlinks without GNU `readlink -f`) ──────────────
_rt_cfg_src="${BASH_SOURCE[0]:-$0}"
while [ -h "$_rt_cfg_src" ]; do
  _rt_cfg_dir="$(cd -P "$(dirname "$_rt_cfg_src")" >/dev/null 2>&1 && pwd)"
  _rt_cfg_src="$(readlink "$_rt_cfg_src")"
  case "$_rt_cfg_src" in /*) ;; *) _rt_cfg_src="$_rt_cfg_dir/$_rt_cfg_src" ;; esac
done
RT_LIBDIR="$(cd -P "$(dirname "$_rt_cfg_src")" >/dev/null 2>&1 && pwd)"
RT_REPODIR="$(cd -P "$RT_LIBDIR/.." >/dev/null 2>&1 && pwd)"

RT_CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/roundtable"
RT_CONFIG_FILE="$RT_CONFIG_DIR/config.env"
RT_INSTALL_URL="https://roundtable.sh/install.sh"

# All heads (display/iteration order) and the subset that take an API key.
ALL_HEADS="claude openai grok glm minimax gemini codex"
KEYED_HEADS="claude openai grok glm minimax gemini"

# ── env-key name for a keyed head ──────────────────────────────────────────
key_for_head() {
  case "$1" in
    claude)  echo "ANTHROPIC_API_KEY" ;;
    openai)  echo "OPENAI_API_KEY" ;;
    grok)    echo "XAI_API_KEY" ;;
    glm)     echo "ZAI_API_KEY" ;;
    minimax) echo "MINIMAX_API_KEY" ;;
    gemini)  echo "GEMINI_API_KEY" ;;
    *)       echo "" ;;
  esac
}

# ════════════════════════════════════════════════════════════════════════════
#  Embedded python3 helpers (passed via `python3 -c` so stdin stays the caller's
#  real stdin — required so a piped key is read, not the heredoc). No single
#  quotes inside, so the whole program can live in a single-quoted bash string.
# ════════════════════════════════════════════════════════════════════════════
RT_SECRET_PY='
import sys
def from_stdin():
    return sys.stdin.readline().rstrip("\r\n")
def masked(prompt):
    try:
        ti = open("/dev/tty", "r"); to = open("/dev/tty", "w")
    except Exception:
        return None
    try:
        import termios, tty
        fd = ti.fileno()
        old = termios.tcgetattr(fd)
        buf = []
        to.write(prompt); to.flush()
        try:
            tty.setraw(fd)
            while True:
                ch = ti.read(1)
                if ch == "" or ch in ("\r", "\n"):
                    break
                if ch == "\x03":
                    termios.tcsetattr(fd, termios.TCSADRAIN, old)
                    to.write("\n"); to.flush()
                    sys.exit(130)
                if ch in ("\x7f", "\b"):
                    if buf:
                        buf.pop(); to.write("\b \b"); to.flush()
                    continue
                if ch == "\x1b":
                    continue
                buf.append(ch); to.write("*"); to.flush()
        finally:
            termios.tcsetattr(fd, termios.TCSADRAIN, old)
            to.write("\n"); to.flush()
        return "".join(buf)
    except Exception:
        return None
prompt = sys.argv[1] if len(sys.argv) > 1 else "Paste key: "
if not sys.stdin.isatty():
    sys.stdout.write(from_stdin())
else:
    v = masked(prompt)
    if v is None:
        import getpass
        try:
            v = getpass.getpass(prompt)
        except Exception:
            v = from_stdin()
    sys.stdout.write(v)
'

RT_UPSERT_PY='
import sys, os
path, key, val = sys.argv[1], sys.argv[2], sys.argv[3]
lines, found = [], False
if os.path.exists(path):
    with open(path) as f:
        for line in f:
            s = line.lstrip()
            head = s.split("=", 1)[0].strip() if "=" in s else None
            if head == key:
                if not found:
                    lines.append(key + "=" + val + "\n"); found = True
                continue
            if not line.endswith("\n"):
                line += "\n"
            lines.append(line)
if not found:
    lines.append(key + "=" + val + "\n")
with open(path, "w") as f:
    f.writelines(lines)
try:
    os.chmod(path, 0o600)
except Exception:
    pass
'

# ── config.env primitives ──────────────────────────────────────────────────
rt_config_ensure_dir() {
  [ -d "$RT_CONFIG_DIR" ] || mkdir -p "$RT_CONFIG_DIR" 2>/dev/null
  chmod 700 "$RT_CONFIG_DIR" 2>/dev/null || true
}

# Value of KEY from config.env only (last match wins). Empty if absent.
rt_cfg_value() {
  local name="$1"
  [ -f "$RT_CONFIG_FILE" ] || return 0
  grep -E "^[[:space:]]*${name}=" "$RT_CONFIG_FILE" 2>/dev/null | tail -n1 | sed -E "s/^[[:space:]]*${name}=//"
}

# Value from the live environment if set, else from config.env.
rt_env_or_config() {
  local name="$1" v=""
  eval "v=\${$name:-}"
  if [ -n "$v" ]; then printf '%s' "$v"; return 0; fi
  rt_cfg_value "$name"
}

# Idempotent KEY=VALUE upsert; keeps file chmod 600.
rt_upsert() {
  local key="$1" val="$2"
  rt_config_ensure_dir
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "$RT_UPSERT_PY" "$RT_CONFIG_FILE" "$key" "$val"
  else
    local tmp; tmp="$(mktemp)"
    [ -f "$RT_CONFIG_FILE" ] && grep -v -E "^[[:space:]]*${key}=" "$RT_CONFIG_FILE" > "$tmp" 2>/dev/null
    printf '%s=%s\n' "$key" "$val" >> "$tmp"
    mv "$tmp" "$RT_CONFIG_FILE"
  fi
  chmod 600 "$RT_CONFIG_FILE" 2>/dev/null || true
}

rt_remove_key() {
  local key="$1" tmp
  [ -f "$RT_CONFIG_FILE" ] || return 0
  tmp="$(mktemp)"
  grep -v -E "^[[:space:]]*${key}=" "$RT_CONFIG_FILE" > "$tmp" 2>/dev/null || true
  mv "$tmp" "$RT_CONFIG_FILE"
  chmod 600 "$RT_CONFIG_FILE" 2>/dev/null || true
}

# Mask a secret value for display: keep at most first 2 + last 2 chars.
mask_value() {
  local v="$1"
  local n=${#v}
  if [ "$n" -le 6 ]; then printf '%s' '******'
  else printf '%s********%s' "${v:0:2}" "${v:$((n-2))}"; fi
}

rt_read_secret() {
  local prompt="$1" v
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "$RT_SECRET_PY" "$prompt"
    return $?
  fi
  printf '%s' "$prompt" >&2
  read -rs v 2>/dev/null
  printf '\n' >&2
  printf '%s' "$v"
}

# ── presence checks ────────────────────────────────────────────────────────
head_key_present() {  # keyed heads only
  local h="$1" v
  v="$(rt_env_or_config "$(key_for_head "$h")")"
  if [ -z "$v" ] && [ "$h" = gemini ]; then v="$(rt_env_or_config GOOGLE_API_KEY)"; fi
  [ -n "$v" ]
}

rt_codex_logged_in() {
  command -v codex >/dev/null 2>&1 || return 1
  local h="${CODEX_HOME:-$HOME/.codex}"
  [ -f "$h/auth.json" ] && return 0
  return 1
}

# ════════════════════════════════════════════════════════════════════════════
#  auth
# ════════════════════════════════════════════════════════════════════════════
rt_print_provider_status() {
  local h mark src
  echo "Providers (✓ = configured, ✗ = missing):"
  echo
  for h in $KEYED_HEADS; do
    src="$(key_for_head "$h")"
    if head_key_present "$h"; then mark="✓"; else mark="✗"; fi
    printf '  %s %-8s %s\n' "$mark" "$h" "$src"
  done
  if rt_codex_logged_in; then
    printf '  %s %-8s %s\n' "✓" "codex" "local codex CLI (logged in)"
  elif command -v codex >/dev/null 2>&1; then
    printf '  %s %-8s %s\n' "✗" "codex" "local codex CLI found — run 'codex login'"
  else
    printf '  %s %-8s %s\n' "✗" "codex" "local codex CLI not installed"
  fi
}

rt_cmd_auth() {
  local provider="${1:-}"
  rt_print_provider_status
  if [ -z "$provider" ]; then
    echo
    printf 'Which provider to configure? [%s]: ' "$(echo "$KEYED_HEADS" | tr ' ' '/')"
    read -r provider 2>/dev/null || provider=""
  fi
  provider="$(printf '%s' "$provider" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]')"
  [ -z "$provider" ] && { echo "No provider selected — nothing changed." >&2; return 1; }

  if [ "$provider" = codex ]; then
    if rt_codex_logged_in; then
      echo "codex is logged in via the local codex CLI — nothing to store here."
    elif command -v codex >/dev/null 2>&1; then
      echo "codex CLI is installed but not logged in. Run:  codex login"
    else
      echo "codex CLI not installed. Install it and run 'codex login' to enable the codex head." >&2
    fi
    return 0
  fi

  local keyname; keyname="$(key_for_head "$provider")"
  if [ -z "$keyname" ]; then
    echo "Unknown provider: $provider (expected one of: $KEYED_HEADS codex)" >&2
    return 1
  fi

  rt_config_ensure_dir
  local secret; secret="$(rt_read_secret "Paste $provider key ($keyname): ")"
  secret="$(printf '%s' "$secret" | tr -d '\r\n')"
  [ -z "$secret" ] && { echo "No key entered — nothing changed." >&2; return 1; }
  rt_upsert "$keyname" "$secret"
  echo "✓ Saved $keyname → $RT_CONFIG_FILE (chmod 600)."
}

# ════════════════════════════════════════════════════════════════════════════
#  models
# ════════════════════════════════════════════════════════════════════════════
rt_cmd_models() {
  rt_config_ensure_dir
  echo "Per-head model id (blank = keep, '-' = reset to engine default):"
  echo
  local h H var cur new
  for h in $ALL_HEADS; do
    [ "$h" = codex ] && continue  # codex uses the local codex CLI's own default model
    H="$(printf '%s' "$h" | tr '[:lower:]' '[:upper:]')"
    var="ROUNDTABLE_${H}_MODEL"
    cur="$(rt_env_or_config "$var")"
    [ -z "$cur" ] && cur="(engine default)"
    printf '  %-8s [%s]: ' "$h" "$cur"
    read -r new 2>/dev/null || new=""
    [ -z "$new" ] && continue
    if [ "$new" = "-" ]; then
      rt_remove_key "$var"; echo "    → $var reset to engine default"
    else
      rt_upsert "$var" "$new"; echo "    → $var=$new"
    fi
  done
}

# ════════════════════════════════════════════════════════════════════════════
#  heads (default-enabled set)
# ════════════════════════════════════════════════════════════════════════════
rt_cmd_heads() {
  rt_config_ensure_dir
  local cur; cur="$(rt_env_or_config ROUNDTABLE_HEADS)"
  [ -z "$cur" ] && cur="(engine default: all)"
  echo "Available heads: $ALL_HEADS"
  echo "Current default: $cur"
  echo
  printf 'Comma-separated heads to enable by default (blank = keep, "-" = clear): '
  local new; read -r new 2>/dev/null || new=""
  [ -z "$new" ] && { echo "Unchanged."; return 0; }
  if [ "$new" = "-" ]; then rt_remove_key ROUNDTABLE_HEADS; echo "Cleared default heads."; return 0; fi
  new="$(printf '%s' "$new" | tr -d '[:space:]' | tr '[:upper:]' '[:lower:]')"
  local h k ok bad="" list
  list="$(printf '%s' "$new" | tr ',' ' ')"
  for h in $list; do
    ok=0
    for k in $ALL_HEADS; do [ "$h" = "$k" ] && ok=1; done
    [ "$ok" -eq 0 ] && bad="$bad $h"
  done
  [ -n "$bad" ] && echo "WARN: unknown head(s):$bad (kept; the engine skips unknowns)" >&2
  rt_upsert ROUNDTABLE_HEADS "$new"
  echo "Set ROUNDTABLE_HEADS=$new"
}

# ════════════════════════════════════════════════════════════════════════════
#  doctor — per-head configured? + fast (<=5s) reachability. Read-only.
# ════════════════════════════════════════════════════════════════════════════
rt_classify_http() {
  case "$1" in
    2*)       printf 'ok (%s)' "$1" ;;
    401|403)  printf 'key rejected (%s)' "$1" ;;
    000|"")   printf 'unreachable' ;;
    *)        printf 'reachable (%s)' "$1" ;;
  esac
}

head_ping() {
  local h="$1" key="$2" code
  command -v curl >/dev/null 2>&1 || { printf 'curl missing'; return; }
  case "$h" in
    claude)
      code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 \
        -H "x-api-key: $key" -H "anthropic-version: 2023-06-01" \
        https://api.anthropic.com/v1/models 2>/dev/null)" ;;
    openai)
      code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 \
        -H "Authorization: Bearer $key" https://api.openai.com/v1/models 2>/dev/null)" ;;
    grok)
      code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 \
        -H "Authorization: Bearer $key" https://api.x.ai/v1/models 2>/dev/null)" ;;
    glm)
      code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 \
        -H "Authorization: Bearer $key" https://api.z.ai/api/paas/v4/models 2>/dev/null)" ;;
    minimax)
      code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 \
        -H "Authorization: Bearer $key" https://api.minimax.io/v1/models 2>/dev/null)" ;;
    gemini)
      code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 \
        "https://generativelanguage.googleapis.com/v1beta/models?key=$key" 2>/dev/null)" ;;
    *) printf 'n/a'; return ;;
  esac
  rt_classify_http "$code"
}

rt_doctor_row() {
  local h="$1" src conf reach keyval
  if [ "$h" = codex ]; then
    src="codex CLI login"
    if rt_codex_logged_in; then conf="✓"; reach="logged in"
    elif command -v codex >/dev/null 2>&1; then conf="✗"; reach="CLI found, not logged in"
    else conf="✗"; reach="CLI not found"; fi
    printf '  %-9s %-22s %-11s %s\n' "$h" "$src" "$conf" "$reach"
    return
  fi
  src="$(key_for_head "$h")"
  keyval="$(rt_env_or_config "$src")"
  if [ -z "$keyval" ] && [ "$h" = gemini ]; then
    keyval="$(rt_env_or_config GOOGLE_API_KEY)"; [ -n "$keyval" ] && src="GOOGLE_API_KEY"
  fi
  if [ -n "$keyval" ]; then
    conf="✓"; reach="$(head_ping "$h" "$keyval")"
  else
    conf="✗"
    if [ "$h" = grok ] && command -v grok >/dev/null 2>&1; then reach="grok CLI present (no key)"
    else reach="— (no key)"; fi
  fi
  printf '  %-9s %-22s %-11s %s\n' "$h" "$src" "$conf" "$reach"
}

rt_cmd_doctor() {
  echo "Roundtable doctor"
  echo "Config: $RT_CONFIG_FILE"
  echo
  printf '  %-9s %-22s %-11s %s\n' "HEAD" "AUTH SOURCE" "CONFIGURED" "REACHABILITY"
  printf '  %-9s %-22s %-11s %s\n' "----" "-----------" "----------" "------------"
  local h
  for h in $ALL_HEADS; do rt_doctor_row "$h"; done
  echo
  echo "(reachability is a ≤5s ping; read-only, nothing is billed)"
  return 0
}

# ════════════════════════════════════════════════════════════════════════════
#  config (show masked / edit)
# ════════════════════════════════════════════════════════════════════════════
rt_print_config_line() {
  local line="$1" key val
  case "$line" in
    ''|\#*) printf '%s\n' "$line"; return ;;
  esac
  case "$line" in
    *=*) key="${line%%=*}"; val="${line#*=}" ;;
    *)   printf '%s\n' "$line"; return ;;
  esac
  key="$(printf '%s' "$key" | tr -d '[:space:]')"
  case "$key" in
    *_API_KEY|*_KEY|*_TOKEN|*_SECRET|*_PASSWORD)
      printf '%s=%s\n' "$key" "$(mask_value "$val")" ;;
    *)
      printf '%s=%s\n' "$key" "$val" ;;
  esac
}

rt_cmd_config() {
  local sub="${1:-}"
  if [ "$sub" = edit ]; then
    rt_config_ensure_dir
    if [ ! -f "$RT_CONFIG_FILE" ]; then : > "$RT_CONFIG_FILE"; chmod 600 "$RT_CONFIG_FILE" 2>/dev/null; fi
    "${EDITOR:-vi}" "$RT_CONFIG_FILE"
    return $?
  fi
  echo "Config: $RT_CONFIG_FILE"
  if [ ! -f "$RT_CONFIG_FILE" ]; then
    echo "(not created yet — run 'roundtable auth' to add a key)"
    return 0
  fi
  echo
  local line
  while IFS= read -r line || [ -n "$line" ]; do
    rt_print_config_line "$line"
  done < "$RT_CONFIG_FILE"
}

# ════════════════════════════════════════════════════════════════════════════
#  version / update
# ════════════════════════════════════════════════════════════════════════════
rt_cmd_version() {
  local v="0.1.0"
  [ -f "$RT_REPODIR/VERSION" ] && v="$(head -n1 "$RT_REPODIR/VERSION" 2>/dev/null | tr -d '[:space:]')"
  [ -z "$v" ] && v="0.1.0"
  echo "roundtable $v"
}

rt_cmd_update() {
  local cmd="curl -fsSL $RT_INSTALL_URL | bash"
  echo "Update Roundtable by re-running the installer:"
  echo
  echo "    $cmd"
  echo
  printf 'Run it now? [y/N]: '
  local ans; read -r ans 2>/dev/null || ans=""
  case "$ans" in
    y|Y|yes|YES) exec bash -c "$cmd" ;;
    *) echo "Skipped." ;;
  esac
}

# ── dispatch (only when executed directly, not when sourced) ────────────────
rt_main() {
  local sub="${1:-}"
  [ $# -gt 0 ] && shift
  case "$sub" in
    auth)    rt_cmd_auth "$@" ;;
    models)  rt_cmd_models "$@" ;;
    heads)   rt_cmd_heads "$@" ;;
    doctor)  rt_cmd_doctor "$@" ;;
    config)  rt_cmd_config "$@" ;;
    version) rt_cmd_version "$@" ;;
    update)  rt_cmd_update "$@" ;;
    *) echo "config.sh: unknown subcommand '$sub' (auth|models|heads|doctor|config|version|update)" >&2; return 2 ;;
  esac
}

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  rt_main "$@"
fi
