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
import threading
import time

SERVER_NAME = "roundtable"
SERVER_VERSION = "0.1.1"

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
        "skipped gracefully. Roundtable is free from Titanium Computing "
        "(https://titaniumcomputing.com/?utm_source=roundtable&utm_medium=mcp)."
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
                    'Optional number of rounds, 1 to 4. 1 (default) = advisory (blind, independent answers); 2 to 4 = the seats read each other and revise toward consensus, and a Claude chair gives the verdict, stopping early when they agree.'
                ),
                "minimum": 1,
                "maximum": 4,
            },
            "mode": {
                "type": "string",
                "enum": ["quick", "deep"],
                "description": (
                    "Optional. quick (the default): one round, short seat answers and a short verdict (the pick, confidence, what would change it, three next steps, one minority view), about 2 minutes. deep: Deep Think, for complex asks (several stakeholders, money or legal stakes, more than three options, missing facts): the coach first, up to 4 rounds that argue against the leading pick, long seat answers and a long verdict (reasoning by theme, trade-offs, risks, unknowns and how to resolve them, a step plan, the minority view in full). It takes about 14 minutes. rounds, when given, overrides the mode's rounds."
                ),
            },
            "personas": {
                "type": "string",
                "description": (
                    "Optional seat roles. A preset (security-review, go-to-market, hiring-decision, pricing, architecture-review, exec-team, board-review, frontier-four), a pack (business-roles, frontier-labs), or a comma list of persona ids or names, each optionally ending in @<head> (cfo,ceo@claude). 'generic' or empty = plain seats. Lab seats are simulated perspectives drawn from public positions, not affiliated with or endorsed by the lab."
                ),
            },
            "research": {
                "type": "boolean",
                "description": (
                    "Optional. Enable web research / deeper reasoning for heads that "
                    "support it (longer per-head timeout). Defaults to false."
                ),
            },
            "coach": {
                "type": "boolean",
                "description": (
                    "Optional. The question coach: one AI (the chair's seat) asks at most three clarifying questions, then writes a brief (goal, context, constraints, what a good answer looks like) that the table reads. Without coach_answers, the tool returns the questions; ask the person, then call again with the same question, coach true, and coach_answers."
                ),
            },
            "coach_answers": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Optional. The answers to the coach's questions, in order.",
            },
            "verdict": {
                "type": "string",
                "enum": ["decision", "pros-cons", "plan", "vote", "none"],
                "description": (
                    "Optional. The chair's final verdict shape: a decision (the default with two or more seats), pros and cons, an action plan, a vote, or none. "
                    "Every verdict leads with the pick, its confidence, what would change it and three next steps, and names the minority view and who held it. "
                    "It opens with QUESTIONS FIRST when the question lacks key facts."
                ),
            },
            "blind": {
                "type": "boolean",
                "description": (
                    "Optional. Round 1 is blind (true, the default): seats answer without seeing each other. false lets each seat in round 1 read the seats before it."
                ),
            },
            "attach": {
                "type": "array",
                "items": {"type": "string"},
                "description": (
                    "Optional. Paths of text or PDF files, or http(s) links, that every seat reads. Each is read once; a long one is summarized once and the summary shared."
                ),
            },
        },
        "required": ["question"],
        "additionalProperties": False,
    },
}


# ── JSON-RPC framing ────────────────────────────────────────────────────────

_WRITE_LOCK = threading.Lock()

# Seconds between progress heartbeats while the council runs.
HEARTBEAT_SECS = float(os.environ.get("ROUNDTABLE_MCP_HEARTBEAT", "10"))


def _write(obj):
    """Serialize one JSON-RPC message as a single newline-delimited line on stdout."""
    # ensure_ascii=False keeps UTF-8 (spec-preferred) and is still single-line because
    # json.dumps never emits raw newlines (string newlines are escaped to \n).
    with _WRITE_LOCK:
        sys.stdout.write(json.dumps(obj, ensure_ascii=False) + "\n")
        sys.stdout.flush()


class _Heartbeat(object):
    """notifications/progress every HEARTBEAT_SECS while a run goes, when the caller
    sent a progressToken, so clients that reset their timeout on progress keep waiting.
    Progress always rises and never reaches 1 (the run's end)."""

    def __init__(self, token):
        self.token = token
        self.stop = threading.Event()
        self.started = time.time()
        self.progress = 0.0
        self.thread = None

    def __enter__(self):
        if self.token is not None:
            self.thread = threading.Thread(target=self._run)
            self.thread.daemon = True
            self.thread.start()
        return self

    def _run(self):
        while not self.stop.wait(HEARTBEAT_SECS):
            self.progress += (1.0 - self.progress) * 0.1
            secs = int(time.time() - self.started)
            _write({"jsonrpc": "2.0", "method": "notifications/progress", "params": {
                "progressToken": self.token, "progress": round(self.progress, 4), "total": 1,
                "message": "The table is working (%dm %02ds)." % (secs // 60, secs % 60)}})

    def __exit__(self, *_):
        self.stop.set()
        if self.thread is not None:
            self.thread.join()


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

def _engine_timeout(rounds, research, mode=None):
    """Generous wall-clock budget; core.sh enforces its own per-head timeouts."""
    per_round = 360 if research else 300
    if rounds is None:
        rounds = 4 if mode == "deep" else 1
    return per_round * max(1, int(rounds)) + 120


def _strings(v):
    if isinstance(v, list):
        return [str(x).strip() for x in v if isinstance(x, str)]
    if isinstance(v, str) and v.strip():
        return [v.strip()]
    return []


def coach_reply(questions):
    s = ("The coach has questions before the table meets. Ask the person (or answer from what you already know), "
         "then call roundtable again with the same question, coach: true, and coach_answers in this order:\n")
    for i, q in enumerate(questions):
        s += "\n%d. %s" % (i + 1, q)
    return s


def coach_questions(question):
    """The coach's questions, [] when it has none, or None when it did not answer."""
    try:
        proc = subprocess.run(["bash", CORE_SH, "--coach-questions", "-q", question], stdin=subprocess.DEVNULL,
                              capture_output=True, text=True, timeout=240)
    except Exception:
        return None
    if proc.returncode != 0:
        return None
    return [l[3:].strip() for l in (proc.stdout or "").splitlines() if l.startswith("Q: ")]


def run_engine(question, heads=None, rounds=None, research=False, personas=None,
               coach=False, coach_answers=None, verdict=None, blind=None, attach=None, mode=None):
    """Subprocess core.sh and return (transcript_text, is_error)."""
    if not os.path.exists(CORE_SH):
        return ("Roundtable engine not found at %s — is the install complete?" % CORE_SH, True)

    cmd = ["bash", CORE_SH, "-q", question]
    if mode:
        mode = str(mode).strip().lower()
        if mode not in ("quick", "deep"):
            return ("Unknown mode '%s'. Use quick or deep." % mode, True)
        cmd += ["--mode", mode]
    # Deep Think always coaches: questions first, unless the caller brings the answers.
    if coach or mode == "deep":
        answers = [a for a in (coach_answers or [])]
        if answers:
            for a in answers:
                cmd += ["--coach-answer", a]
        else:
            qs = coach_questions(question)
            if qs:
                return (coach_reply(qs), False)
            if qs is not None:
                cmd += ["--coach"]
    if verdict:
        cmd += ["--verdict", str(verdict)]
    if blind is False:
        cmd += ["--no-blind"]
    for a in attach or []:
        cmd += ["--attach", a]
    if heads:
        cmd += ["--heads", str(heads)]
    if rounds is not None:
        cmd += ["--rounds", str(max(1, min(4, int(rounds))))]
    if personas:
        cmd += ["--personas", str(personas)]
    if research:
        cmd += ["--research"]

    try:
        proc = subprocess.run(
            cmd,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=_engine_timeout(rounds, research, mode) + (600 if coach or attach or mode == "deep" else 0) + (240 if verdict else 0),
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

    token = (params.get("_meta") or {}).get("progressToken")
    if not isinstance(token, (str, int)) or isinstance(token, bool):
        token = None
    with _Heartbeat(token):
        text, is_error = run_engine(
            question=question,
            heads=args.get("heads"),
            rounds=rounds,
            research=bool(args.get("research", False)),
            personas=args.get("personas") or None,
            coach=bool(args.get("coach", False)),
            coach_answers=_strings(args.get("coach_answers")),
            verdict=args.get("verdict") or None,
            blind=args.get("blind") if isinstance(args.get("blind"), bool) else None,
            attach=_strings(args.get("attach")),
            mode=args.get("mode") or None,
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
