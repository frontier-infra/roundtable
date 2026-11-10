#!/usr/bin/env bash
# Build and push frontierinfra/jebadiah:4b, :9b and :27b to the Ollama library.
#
# Needs: ollama and the hf CLI on PATH, a work folder with room for the GGUFs
# (about 26 GB for all three), and this machine's Ollama public key added to the
# frontierinfra account on ollama.com (Settings, Ollama keys). Find the key with
#   cat ~/.ollama/id_ed25519.pub
# (on Atlas the server runs as the ollama user: /mnt/llm-models/ollama-archive/.ollama/id_ed25519.pub).
#
# On Atlas, put the Hugging Face cache on /data too: HF_HOME=/data/code/<dir>/hf.
# Checked 2026-11-10 on Atlas (ollama 0.13.5): the 4B builds with --dry-run.
#
# Usage: packaging/ollama/publish.sh WORKDIR [4b 9b 27b]
#   --dry-run   build and check, do not push
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
DRY=0
ARGS=()
for a in "$@"; do
  if [ "$a" = "--dry-run" ]; then DRY=1; else ARGS+=("$a"); fi
done
WORK="${ARGS[0]:?usage: publish.sh WORKDIR [4b 9b 27b] [--dry-run]}"
SIZES=("${ARGS[@]:1}")
[ ${#SIZES[@]} -eq 0 ] && SIZES=(4b 9b 27b)

repo_for() {
  case "$1" in
    4b) echo "jebadiah-4b-v2-GGUF jebadiah-4b-v2-Q4_K_M.gguf" ;;
    9b) echo "jebadiah-9b-v2-1-GGUF jebadiah-9b-v2-1-Q4_K_M.gguf" ;;
    27b) echo "jebadiah-27b-v2-1-GGUF jebadiah-27b-v2-1-Q4_K_M.gguf" ;;
    *) echo "unknown size $1" >&2; exit 2 ;;
  esac
}

mkdir -p "$WORK"
for size in "${SIZES[@]}"; do
  read -r repo gguf <<<"$(repo_for "$size")"
  dir="$WORK/$size"
  mkdir -p "$dir"
  # Always the full frontier-infra address: another account has a similar name.
  hf download "frontier-infra/$repo" "$gguf" --local-dir "$dir"
  cp "$HERE/Modelfile.$size" "$dir/Modelfile"
  tag="frontierinfra/jebadiah:$size"
  ollama create "$tag" -f "$dir/Modelfile"
  ollama show "$tag" --modelfile | grep -q "^FROM " || { echo "create failed for $tag" >&2; exit 1; }
  if [ "$DRY" = 1 ]; then
    echo "built $tag (dry run, not pushed)"
  else
    ollama push "$tag"
    echo "pushed $tag"
  fi
done
if [ "$DRY" = 0 ] && printf '%s\n' "${SIZES[@]}" | grep -qx 9b; then
  ollama cp frontierinfra/jebadiah:9b frontierinfra/jebadiah:latest
  ollama push frontierinfra/jebadiah:latest
fi
echo "Paste packaging/ollama/README.md into the model's page on ollama.com."
