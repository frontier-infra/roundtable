#!/usr/bin/env python3
"""roundtable mcp — hand-rolled stdio MCP server (JSON-RPC 2.0, python3 stdlib only).

Exposes the Roundtable council engine (sibling ``core.sh``) as a single MCP tool so
any coding harness (Claude Code, Cursor, Codex CLI, ...) can convene the council.

Subcommands
-----------
  python3 mcp.py serve       Run the stdio MCP server (newline-delimited JSON-RPC).
  python3 mcp.py config      Print the JSON block to paste into a harness's MCP config.
  python3 mcp.py --selftest  Pipe initialize -> tools/list -> tools/call through a live
                             ``serve`` subprocess and print the JSON-RPC responses.

Transport (confirmed against MCP spec rev 2025-06-18, basic/transports):
  * JSON-RPC 2.0, UTF-8 encoded.
  * One JSON message per line on stdin/stdout; messages MUST NOT contain embedded
    newlines (json.dumps emits none; we append a single '\\n').
  * stdout carries ONLY MCP messages; diagnostics go to stderr.

Manual test one-liner (no key required — an "unavailable" transcript still round-trips):
  printf '%s\\n' \\
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' \\
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \\
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \\
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"roundtable","arguments":{"question":"Say OK","heads":"claude"}}}' \\
    | python3 lib/mcp.py serve
"""

import json
import os
import subprocess
import sys

SERVER_NAME = "roundtable"
SERVER_VERSION = "0.1.0"

# Latest protocol revision we author against. We echo the client's requested version
# when it is one we recognize (per spec version-negotiation), else advertise this one.
DEFAULT_PROTOCOL_VERSION = "2025-06-18"
KNOWN_PROTOCOL_VERSIONS = {"2025-06-18", "2025-03-26", "2024-11-05"}

# JSON-RPC 2.0 standard error codes.
PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603

HERE = os.path.dirname(os.path.abspath(__file__))
CORE_SH = os.path.join(HERE, "core.sh")  # the council engine, resolved next to us

TOOL = {
    "name": "roundtable",
    "description": (
        "Convene a council of frontier AI models (Grok, Codex/OpenAI, GLM, MiniMax, "
        "Claude, Gemini) on a question and return their answers as a markdown "
        "transcript. With rounds>1 the council deliberates across rounds until a "
        "Claude chair declares consensus. Heads with no configured key/CLI are "
        "skipped gracefully."
    ),
    "inputSchema": {
        "type": "object",
        "properties": {
            "question": {
                "type": "string",
                "description": "The question to put to the council.",
            },
            "heads": {
                "type": "string",
                "description": (
                    "Optional comma-separated subset of heads to convene "
                    "(any of: grok,codex,openai,glm,minimax,claude,gemini). "
                    "Defaults to all configured heads."
                ),
            },
            "rounds": {
                "type": "integer",
                "description": (
                    "Optional number of deliberation rounds. 1 (default) = advisory "
                    "(blind, independent answers); N>=2 = chaired deliberation to "
                    "consensus, stopping early when the chair declares consensus."
                ),
                "minimum": 1,
            },
            "research": {
                "type": "boolean",
                "description": (
                    "Optional. Enable web research / deeper reasoning for heads that "
                    "support it (longer per-head timeout). Defaults to false."
                ),
            },
        },
        "required": ["question"],
        "additionalProperties": False,
    },
}


# ── JSON-RPC framing ────────────────────────────────────────────────────────

def _write(obj):
    """Serialize one JSON-RPC message as a single newline-delimited line on stdout."""
    # ensure_ascii=False keeps UTF-8 (spec-preferred) and is still single-line because
    # json.dumps never emits raw newlines (string newlines are escaped to \n).
    sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _result(req_id, result):
    _write({"jsonrpc": "2.0", "id": req_id, "result": result})


def _error(req_id, code, message, data=None):
    err = {"code": code, "message": message}
    if data is not None:
        err["data"] = data
    _write({"jsonrpc": "2.0", "id": req_id, "error": err})


def _log(msg):
    sys.stderr.write("[roundtable-mcp] %s\n" % msg)
    sys.stderr.flush()


# ── engine bridge ───────────────────────────────────────────────────────────

def _engine_timeout(rounds, research):
    """Generous wall-clock budget; core.sh enforces its own per-head timeouts."""
    per_round = 360 if research else 300
    return per_round * max(1, int(rounds or 1)) + 120


def run_engine(question, heads=None, rounds=None, research=False):
    """Subprocess core.sh and return (transcript_text, is_error)."""
    if not os.path.exists(CORE_SH):
        return ("Roundtable engine not found at %s — is the install complete?" % CORE_SH, True)

    cmd = ["bash", CORE_SH, "-q", question]
    if heads:
        cmd += ["--heads", str(heads)]
    if rounds is not None:
        cmd += ["--rounds", str(int(rounds))]
    if research:
        cmd += ["--research"]

    try:
        proc = subprocess.run(
            cmd,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=_engine_timeout(rounds, research),
        )
    except subprocess.TimeoutExpired:
        return ("Roundtable timed out while convening the council.", True)
    except Exception as exc:  # pragma: no cover - defensive
        return ("Failed to run the Roundtable engine: %s" % exc, True)

    transcript = (proc.stdout or "").strip()
    if transcript:
        # Engine printed a transcript. Non-zero exit just means no head answered
        # (e.g. no keys configured) — the transcript is still the useful payload.
        return (transcript, False)

    # No transcript at all: surface stderr as a genuine error.
    diag = (proc.stderr or "").strip() or "(no output from engine)"
    return ("Roundtable produced no transcript (rc=%d): %s" % (proc.returncode, diag), True)


# ── method handlers ─────────────────────────────────────────────────────────

def handle_initialize(req_id, params):
    requested = (params or {}).get("protocolVersion")
    version = requested if requested in KNOWN_PROTOCOL_VERSIONS else DEFAULT_PROTOCOL_VERSION
    _result(req_id, {
        "protocolVersion": version,
        "capabilities": {"tools": {"listChanged": False}},
        "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
        "instructions": (
            "Call the 'roundtable' tool with a question to convene a council of "
            "frontier models. Use rounds>1 for chaired deliberation to consensus."
        ),
    })


def handle_tools_list(req_id, _params):
    _result(req_id, {"tools": [TOOL]})


def handle_tools_call(req_id, params):
    params = params or {}
    name = params.get("name")
    if name != "roundtable":
        _error(req_id, INVALID_PARAMS, "Unknown tool: %r" % name)
        return
    args = params.get("arguments") or {}
    question = args.get("question")
    if not isinstance(question, str) or not question.strip():
        _error(req_id, INVALID_PARAMS, "Missing required argument: 'question' (non-empty string)")
        return

    rounds = args.get("rounds")
    if rounds is not None:
        try:
            rounds = int(rounds)
        except (TypeError, ValueError):
            _error(req_id, INVALID_PARAMS, "'rounds' must be an integer")
            return

    text, is_error = run_engine(
        question=question,
        heads=args.get("heads"),
        rounds=rounds,
        research=bool(args.get("research", False)),
    )
    _result(req_id, {"content": [{"type": "text", "text": text}], "isError": is_error})


def handle_ping(req_id, _params):
    _result(req_id, {})


REQUEST_HANDLERS = {
    "initialize": handle_initialize,
    "tools/list": handle_tools_list,
    "tools/call": handle_tools_call,
    "ping": handle_ping,
}


def dispatch(msg):
    """Route one parsed JSON-RPC object. Notifications (no id) get no response."""
    if not isinstance(msg, dict) or msg.get("jsonrpc") != "2.0":
        # Can't reliably reply without a valid envelope; if there's an id, error on it.
        rid = msg.get("id") if isinstance(msg, dict) else None
        if rid is not None:
            _error(rid, INVALID_REQUEST, "Invalid JSON-RPC 2.0 request")
        return

    method = msg.get("method")
    req_id = msg.get("id")  # absent -> this is a notification
    is_notification = "id" not in msg

    if not isinstance(method, str):
        if not is_notification:
            _error(req_id, INVALID_REQUEST, "Missing or invalid 'method'")
        return

    # Notifications: never respond. We only need to swallow 'initialized' et al.
    if is_notification:
        return

    handler = REQUEST_HANDLERS.get(method)
    if handler is None:
        _error(req_id, METHOD_NOT_FOUND, "Method not found: %s" % method)
        return
    try:
        handler(req_id, msg.get("params"))
    except Exception as exc:  # never crash the server on a single bad call
        _log("handler error for %s: %s" % (method, exc))
        _error(req_id, INTERNAL_ERROR, "Internal error: %s" % exc)


def serve():
    """Read newline-delimited JSON-RPC from stdin until EOF; reply on stdout."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            _error(None, PARSE_ERROR, "Parse error")
            continue
        dispatch(msg)
    return 0


# ── config + selftest ───────────────────────────────────────────────────────

def print_config():
    block = {"mcpServers": {SERVER_NAME: {"command": "roundtable", "args": ["mcp", "serve"]}}}
    print(json.dumps(block, indent=2))
    print()
    print("# Paste the block above into your harness's MCP config "
          "(e.g. ~/.cursor/mcp.json), or run `roundtable install` to auto-wire "
          "detected harnesses.")
    return 0


def selftest():
    """Drive a live `serve` subprocess through the standard handshake + a tool call."""
    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize",
         "params": {"protocolVersion": DEFAULT_PROTOCOL_VERSION, "capabilities": {},
                    "clientInfo": {"name": "selftest", "version": "0"}}},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
        {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
         "params": {"name": "roundtable", "arguments": {"question": "Say OK in 3 words", "heads": "claude"}}},
    ]
    payload = "".join(json.dumps(r) + "\n" for r in requests)
    proc = subprocess.run(
        [sys.executable, os.path.abspath(__file__), "serve"],
        input=payload, capture_output=True, text=True,
        timeout=_engine_timeout(1, False) + 30,
    )
    sys.stderr.write(proc.stderr)
    print("=== selftest: JSON-RPC responses (one per line) ===")
    ok = True
    for line in proc.stdout.splitlines():
        if not line.strip():
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            print("!! non-JSON line on stdout (spec violation): %r" % line)
            ok = False
            continue
        print(json.dumps(obj, indent=2, ensure_ascii=False))
    print("=== selftest:", "PASS" if ok and proc.returncode == 0 else "CHECK OUTPUT", "===")
    return 0 if ok else 1


USAGE = "usage: mcp.py {serve|config|--selftest}\n"


def main(argv):
    args = list(argv[1:])
    # Tolerate being invoked as `mcp.py mcp serve` (dispatcher passthrough).
    if args and args[0] == "mcp":
        args = args[1:]
    cmd = args[0] if args else ""

    if cmd == "serve":
        try:
            return serve()
        except (BrokenPipeError, KeyboardInterrupt):
            return 0
    if cmd == "config":
        return print_config()
    if cmd in ("--selftest", "selftest"):
        return selftest()

    sys.stderr.write(USAGE)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
