#!/usr/bin/env python3
"""Parity test: the bash engine (lib/core.sh) and the Rust engine against one fake provider.

Runs both engines offline, with the OpenAI-compatible heads pointed at a local fake
server and a fake `codex` CLI script first on PATH (a CLI head: both engines must call
it headless with the same flags), in advisory mode and in 2, 3 and 4 round
deliberations (no chair key, so both report the chair as unavailable and run every
round). Then the same with personas: a preset, named seats pinned to an AI, and a lab
seat, so the seat brief, the label and the lab label must match too. The fake answers
with the seat it was told it sits in. Compares every section and body. Exit 0 on a match.

Usage: python3 desktop/scripts/parity.py <path to the Rust engine binary>
"""
import http.server, json, os, re, subprocess, sys, tempfile, threading

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
CORE = os.path.join(REPO, "lib", "core.sh")
HEADS = "grok,codex,openai,glm,minimax,gemini"

# A fake codex CLI. It answers in the -o file, as `codex exec -o FILE` does, and echoes
# the flags it got so a flag drift between the engines fails the comparison.
FAKE_CODEX = r"""#!/bin/sh
out=""; flags=""
while [ $# -gt 1 ]; do
  if [ "$1" = "-o" ]; then out="$2"; shift 2; continue; fi
  flags="$flags $1"; shift
done
case "$1" in *"COUNCIL DELIBERATION"*) tag=revised;; *) tag=first;; esac
seat=$(printf '%s' "$1" | sed -n 's/^You sit at this roundtable as \(.*\)\.$/ as \1/p' | head -1)
printf '%s answer from codex%s (flags:%s)\n' "$tag" "$seat" "$flags" > "$out"
echo "codex progress on stderr" >&2
"""


class Fake(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        prompt = body["messages"][0]["content"]
        tag = "revised" if "COUNCIL DELIBERATION" in prompt else "first"
        m = re.search(r"^You sit at this roundtable as (.*)\.$", prompt, re.M)
        seat = f" as {m.group(1)}" if m else ""
        if "Do not claim to speak for" in prompt:
            seat += " (lab rule)"
        text = f"{tag} answer from {body['model']}{seat}"
        if body["model"].startswith("MiniMax"):
            text = "<think>hidden</think>" + text
        out = json.dumps({"choices": [{"message": {"content": text}}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)

    def log_message(self, *a):
        pass


def env_for(home, url):
    fakebin = os.path.join(home, "fakebin")
    os.makedirs(fakebin, exist_ok=True)
    codex = os.path.join(fakebin, "codex")
    with open(codex, "w") as f:
        f.write(FAKE_CODEX)
    os.chmod(codex, 0o755)
    e = {"HOME": home, "PATH": fakebin + ":/usr/bin:/bin:/usr/sbin:/sbin", "TMPDIR": tempfile.gettempdir(),
         "ROUNDTABLE_KEYSTORE": "memory", "ROUNDTABLE_DATA_DIR": os.path.join(home, "data"),
         "ROUNDTABLE_NO_SHELL_PATH": "1", "ROUNDTABLE_PATH_ONLY": "1"}
    for k in ("XAI_API_KEY", "OPENAI_API_KEY", "ZAI_API_KEY", "MINIMAX_API_KEY", "GEMINI_API_KEY"):
        e[k] = "test"
    for k in ("GROK_XAI", "OPENAI", "GLM", "MINIMAX", "GEMINI"):
        e[f"ROUNDTABLE_{k}_URL"] = url
    return e


NORMALIZE = [
    ("\u2014", "-"),
    ("_(unavailable - skipped)_", "_(unavailable, skipped)_"),
]


def sections(transcript):
    """[(heading, body)] for every ## and ### heading, ignoring the title and the footer."""
    for a, b in NORMALIZE:
        transcript = transcript.replace(a, b)
    out, head, body = [], None, []
    for line in transcript.splitlines():
        if line.startswith("## ") or line.startswith("### "):
            if head is not None:
                out.append((head, "\n".join(body).strip()))
            head, body = line, []
        elif line == "---":
            if head is not None:
                out.append((head, "\n".join(body).strip()))
            head, body = None, []
        elif head is not None:
            body.append(line)
    if head is not None:
        out.append((head, "\n".join(body).strip()))
    # The chair-unavailable note is worded differently on purpose (no em dashes).
    return [(h, re.sub(r"_\(chair unavailable.*\)_", "_(chair unavailable)_", b)) for h, b in out]


def main():
    rust = os.path.abspath(sys.argv[1])
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fake)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{srv.server_address[1]}/v1/chat/completions"
    ok = True
    cases = [(r, None) for r in ("1", "2", "3", "4")]
    cases += [("1", "pricing"), ("2", "cfo@glm,The NVIDIA view,generic@openai,Head of Sales"), ("3", "security-review")]
    for rounds, personas in cases:
        name = f"rounds={rounds}" + (f" personas={personas}" if personas else "")
        with tempfile.TemporaryDirectory() as home:
            env = env_for(home, url)
            args = ["--heads", HEADS, "--rounds", rounds, "-q", "Bread or garden?"]
            if personas:
                args += ["--personas", personas]
            a = subprocess.run(["bash", CORE, *args], env=env, capture_output=True, text=True, timeout=120).stdout
            b = subprocess.run([rust, "ask", "--no-history", *args], env=env, capture_output=True, text=True, timeout=120).stdout
        sa, sb = sections(a), sections(b)
        codex = [x for x in sa if "Codex" in x[0]]
        if not personas and (not codex or "answer from codex (flags: exec --skip-git-repo-check -s read-only -c tools.web_search=false)" not in codex[0][1]):
            ok = False
            print(f"parity {name}: FAIL, the codex CLI head did not answer through the fake CLI: {codex}")
        want_rounds = int(rounds) if rounds != "1" else 0
        got_rounds = sum(1 for h, _ in sa if h.startswith("## ") and " Round " in h)
        if got_rounds != want_rounds:
            ok = False
            print(f"parity {name}: FAIL, bash ran {got_rounds} rounds, wanted {want_rounds}")
        if personas:
            seated = [b for h, b in sa if " as " in b]
            if not seated:
                ok = False
                print(f"parity {name}: FAIL, no seat read its persona brief")
            if "NVIDIA" in personas and not any("Not affiliated with or endorsed by NVIDIA." in b and "(lab rule)" in b for _, b in sa):
                ok = False
                print(f"parity {name}: FAIL, the lab seat lost its label or its rule")
        if sa == sb and sa:
            print(f"parity {name}: PASS ({len(sa)} sections)")
        else:
            ok = False
            print(f"parity {name}: FAIL")
            for x, y in zip(sa, sb):
                if x != y:
                    print("  bash:", x)
                    print("  rust:", y)
            if len(sa) != len(sb):
                print(f"  section counts differ: bash {len(sa)}, rust {len(sb)}")
    srv.shutdown()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
