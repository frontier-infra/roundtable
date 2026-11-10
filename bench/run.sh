#!/usr/bin/env bash
# Run the usefulness bench. Flags pass through to bench/run.py (see --help).
# RT_BIN picks the roundtable command; default is `roundtable` on PATH.
set -euo pipefail
exec python3 "$(cd "$(dirname "$0")" && pwd)/run.py" "$@"
