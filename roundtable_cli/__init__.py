"""Thin console-script launcher for Roundtable.

`pip install .` / `uv tool install .` expose a `roundtable` command that simply
execs the bundled bash engine (bin/roundtable), which sources lib/*. The engine
and its libs are bundled as package data under roundtable_cli/_engine/ (see
pyproject.toml [tool.hatch.build.targets.wheel.force-include]); the bin/ and
lib/ sibling layout is preserved so the dispatcher's `../lib` resolution works.

Keep this tiny: all real behavior lives in the bash engine.
"""

import os
import shutil
import sys
from pathlib import Path

__version__ = "0.1.0"

_ENGINE = Path(__file__).resolve().parent / "_engine" / "bin" / "roundtable"


def engine_path() -> Path:
    """Absolute path to the bundled dispatcher."""
    return _ENGINE


def main() -> "int":
    engine = engine_path()
    if not engine.exists():
        sys.stderr.write(
            "roundtable: bundled engine not found at %s\n"
            "The pip package may be built incorrectly — reinstall, or use the\n"
            "curl installer:  curl -fsSL https://roundtable.sh/install.sh | bash\n"
            % engine
        )
        return 1

    # Exec via bash for portability: wheels do not reliably preserve the
    # executable bit on data files, so we never rely on it. This replaces the
    # current process, so the engine owns stdin/stdout/stderr and the exit code.
    bash = shutil.which("bash") or "/bin/bash"
    argv = [bash, str(engine), *sys.argv[1:]]
    try:
        os.execv(bash, argv)
    except OSError as exc:  # pragma: no cover - exec almost never returns
        sys.stderr.write("roundtable: failed to launch engine: %s\n" % exc)
        return 1
    return 0  # unreachable on success


if __name__ == "__main__":
    raise SystemExit(main())
