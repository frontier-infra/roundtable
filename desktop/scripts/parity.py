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
import hashlib, http.server, json, os, re, subprocess, sys, tempfile, threading

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


# What both engines run codex with: no instructions, memory, servers, commands or web (see clis.rs).
CODEX_FLAGS = ("answer from codex (flags: exec --ignore-user-config --skip-git-repo-check --ephemeral --ignore-rules -s read-only -c project_doc_max_bytes=0 "
               + " ".join(f"-c features.{f}=false" for f in ("shell_tool", "code_mode_host", "memories", "apps", "plugins", "hooks", "browser_use", "computer_use",
                                                            "in_app_browser", "multi_agent", "multi_agent_v2", "skill_search", "tool_suggest", "sleep_tool"))
               + ' -c tools.web_search=false -c web_search="disabled")')


def sha8(s):
    return hashlib.sha256(s.encode()).hexdigest()[:8]


PAGE = "<html><head><title>t</title><style>x{}</style></head><body><h1>Launch plan</h1><p>Ship in March &amp; tell partners.</p><script>no()</script></body></html>"


def chair_reply(prompt):
    """The fake chair: one fixed answer per kind of chair prompt. Hashes prove both engines sent the same prompt."""
    if "help the person sharpen" in prompt:
        return "Q: What is the budget?\n2. Q: Who decides?"
    if "Rewrite the person's question into a brief" in prompt:
        answers = "; ".join(re.findall(r"^A: (.*)$", prompt, re.M)) or "no answers"
        return f"GOAL: Pick one for the weekend.\nCONTEXT: {answers}.\nCONSTRAINTS: None given\nGOOD ANSWER: One pick and why. p:{sha8(prompt)}\nDEEP THINK: yes: Money is at stake."
    if "Pick the seats" in prompt:
        return f"SEATS: cfo, nobody, ceo\nWHY: Money and direction. p:{sha8(prompt)}"
    if "Every councilor will read your summary" in prompt:
        src = re.search(r"^SOURCE: (.*)$", prompt, re.M).group(1)
        return f"- the gist of {src} p:{sha8(prompt)}"
    if "decide whether the person must answer questions first" in prompt:
        return "Q: What is the workload?\nQ: How many users?" if "database" in prompt else "NONE"
    if "Write the final verdict" in prompt:
        depth = "deep" if "THE MINORITY VIEW IN FULL" in prompt else "quick"
        depth += " asks-first" if "QUESTIONS FIRST" in prompt else ""
        return f"DECISION: Start the garden.\nCONFIDENCE: High (80%)\n**WHY**\n- p:{sha8(prompt)} {depth}\nMINORITY VIEW: Bake bread first.\nHELD BY: GLM"
    return "VERDICT: CONTINUE\n**AGREED**\n- Both are cheap.\n\n**REMAINING DISAGREEMENTS**\n- Which first."


class Fake(http.server.BaseHTTPRequestHandler):
    def _send(self, ctype, out):
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)

    def do_GET(self):
        self._send("text/html; charset=utf-8", PAGE.encode())

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        prompt = body["messages"][0]["content"]
        if self.path.endswith("/v1/messages"):
            return self._send("application/json", json.dumps({"content": [{"type": "text", "text": chair_reply(prompt)}]}).encode())
        tag = "revised" if "COUNCIL DELIBERATION" in prompt else "first"
        m = re.search(r"^You sit at this roundtable as (.*)\.$", prompt, re.M)
        seat = f" as {m.group(1)}" if m else ""
        if "Do not claim to speak for" in prompt:
            seat += " (lab rule)"
        text = f"{tag} answer from {body['model']}{seat}"
        if "ANSWERS SO FAR" in prompt:
            text += " (saw earlier seats)"
        # The mode's LENGTH section closes every seat prompt: Quick about 250 words, Deep Think 800.
        cap = re.search(r"## LENGTH\n.*?about (\d+) words", prompt, re.S)
        if cap and prompt.rstrip("\n").endswith("."):
            text += f" len:{cap.group(1)}"
        # The new prompt sections: both engines must send the very same prompt.
        if "COUNCIL DELIBERATION" not in prompt and any(k in prompt for k in ("## BRIEF", "## ATTACHMENTS", "EARLIER IN THIS SESSION", "ANSWERS SO FAR", "## OPEN QUESTIONS")):
            text += f" p:{sha8(prompt)}"
        # Round 2 with a chair: both engines send the very same deliberation prompt.
        if "THE CHAIR'S READING OF ROUND" in prompt:
            text += f" d:{sha8(prompt)}"
        if body["model"].startswith("MiniMax"):
            text = "<think>hidden</think>" + text
        self._send("application/json", json.dumps({"choices": [{"message": {"content": text}}]}).encode())

    def log_message(self, *a):
        pass


def env_for(home, url, chair=False):
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
    if chair:
        e["ANTHROPIC_API_KEY"] = "test"
        e["ROUNDTABLE_ANTHROPIC_URL"] = url.replace("/chat/completions", "/messages")
    return e


def rc7_cases(rust, url, base):
    """The question coach, suggested seats, an open first round, verdicts, follow-ups and attachments."""
    ok = True
    with tempfile.TemporaryDirectory() as home:
        files = os.path.join(home, "files")
        os.makedirs(files)
        w = lambda n, t: open(os.path.join(files, n), "w").write(t) and os.path.join(files, n)
        notes = w("notes.md", "Budget: 40 thousand.\r\n\r\n\r\nDeadline: March.  \n")
        long = w("long.txt", "A long report line.\n" * 1000)
        prior = w("prior.md", "# Round Table\n\n_Question:_ Which first?\n\n## Claude\n\nThe garden.\n")
        brief = w("brief.md", "\n- **Goal:** Raise prices or not.\n- **Constraints:** Keep churn under 2%.\n\n")
        runs = [
            ("coach+open+verdict+attach+follow-up", True, {}, ["--heads", "openai,glm,minimax", "--rounds", "1", "--coach", "--coach-answer", "About 40 thousand.",
              "--verdict", "decision", "--no-blind", "--attach", notes, "--attach", long, "--attach", base + "/page.html", "--follow-up", prior, "-q", "Bread or garden?"]),
            ("brief+auto seats+vote, 2 rounds", True, {}, ["--heads", "openai,glm", "--rounds", "2", "--brief", brief, "--verdict", "vote", "--personas", "auto", "-q", "Raise prices?"]),
            ("auto seats by words, no chair", False, {}, ["--heads", "openai,glm", "--personas", "auto", "-q", "Should we raise prices? Budget and revenue matter."]),
            ("pros-cons and plan, no chair", False, {}, ["--heads", "openai,glm", "--rounds", "2", "--verdict", "plan", "-q", "Bread or garden?"]),
            ("clarify first, default verdict, 2 rounds", True, {}, ["--heads", "openai,glm", "--rounds", "2", "-q", "Which database for our app?"]),
            ("quick: clarify alongside one round", True, {}, ["--heads", "openai,glm", "--mode", "quick", "--rounds", "1", "-q", "Which database for our app?"]),
            ("deep think: the coach always, long verdict", True, {}, ["--heads", "openai,glm", "--mode", "deep", "--rounds", "2", "-q", "Raise prices?"]),
        ]
        for name, chair, extra, args in runs:
            env = {**env_for(home, url, chair), **extra}
            a = subprocess.run(["bash", CORE, *args], env=env, capture_output=True, text=True, timeout=180)
            b = subprocess.run([rust, "ask", "--no-history", *args], env=env, capture_output=True, text=True, timeout=180)
            sa, sb = sections(a.stdout), sections(b.stdout)
            checks = []
            if name.startswith("coach"):
                checks = [("## Brief", "About 40 thousand."), ("Verdict (decision)", "MINORITY VIEW: Bake bread first."), ("MiniMax", "(saw earlier seats)")]
            elif name.startswith("brief"):
                checks = [("## Brief", "Keep churn under 2%."), ("Verdict (vote)", "HELD BY: GLM"), ("CFO", "as CFO"), ("CEO", "as CEO")]
            elif name.startswith("auto"):
                checks = [("CFO", "as CFO")]
            elif name.startswith("clarify"):
                # Quick: two rounds back to back, the chair asked alongside round 1; only the verdict reads its questions.
                checks = [("Verdict (decision)", "p:"), ("Verdict (decision)", "asks-first"), ("Round 2", ""), ("OpenAI", "len:250")]
            elif name.startswith("quick"):
                checks = [("Verdict (decision)", "p:"), ("Verdict (decision)", " quick asks-first"), ("OpenAI", "len:250")]
            elif name.startswith("deep"):
                checks = [("## Brief", "Pick one for the weekend."), ("Verdict (decision)", " deep"), ("OpenAI", "len:800"), ("Round 2", ""), ("OpenAI", "d:")]
            else:
                checks = [("Verdict (action plan)", "verdict unavailable")]
            for head, want in checks:
                if not any(head in h and want in bd for h, bd in sa):
                    ok = False
                    print(f"parity {name}: FAIL, bash has no {head!r} section with {want!r}")
                    print("  bash stderr:", a.stderr[-600:])
            title = lambda t: next((l for l in t.splitlines() if l.startswith("# ")), "").replace("\u2014", "-")
            if name.startswith(("quick", "deep")):
                want = "- Deep Think" if name.startswith("deep") else "- Quick"
                if not title(a.stdout).endswith(want) or title(a.stdout) != title(b.stdout):
                    ok = False
                    print(f"parity {name}: FAIL, titles {title(a.stdout)!r} and {title(b.stdout)!r} should both end {want!r}")
                if any("DEEP THINK" in bd for h, bd in sa if h == "## Brief"):
                    ok = False
                    print(f"parity {name}: FAIL, the coach's Deep Think line leaked into the brief")
            if name.startswith("quick") and any("p:" in bd for h, bd in sa if "OpenAI" in h):
                ok = False
                print(f"parity {name}: FAIL, a one-round Quick seat waited for the open questions")
            if name.startswith("coach") and not any("p:" in bd for h, bd in sa if "OpenAI" in h):
                ok = False
                print(f"parity {name}: FAIL, the seats did not read the new prompt sections")
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
                    print("  bash stderr:", a.stderr[-400:])
                    print("  rust stderr:", b.stderr[-400:])
        env = env_for(home, url, True)
        a = subprocess.run(["bash", CORE, "--coach-questions", "-q", "Bread or garden?"], env=env, capture_output=True, text=True, timeout=60).stdout
        b = subprocess.run([rust, "ask", "--coach-questions", "-q", "Bread or garden?"], env=env, capture_output=True, text=True, timeout=60).stdout
        if a == b == "Q: What is the budget?\nQ: Who decides?\n":
            print("parity coach questions: PASS")
        else:
            ok = False
            print(f"parity coach questions: FAIL\n  bash: {a!r}\n  rust: {b!r}")
    return ok


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
        if not personas and (not codex or CODEX_FLAGS not in codex[0][1]):
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
    ok = rc7_cases(rust, url, url.rsplit("/v1/", 1)[0]) and ok
    srv.shutdown()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
