#!/usr/bin/env python3
"""coach.py: the question coach, the suggested panel and shared attachments for the bash
engine. Same prompts and rules as the Rust engine (desktop/engine/src/coach.rs, attach.rs).
Standard library only. core.sh makes the chair calls; this file builds and reads the text.

  coach.py questions-prompt QUESTION            the prompt that asks for clarifying questions
  coach.py parse-questions < REPLY              "Q: ..." lines, or NONE
  coach.py brief-prompt QUESTION QAFILE         QAFILE: one "question<TAB>answer" per line
  coach.py parse-brief QUESTION < REPLY         the brief as Markdown
  coach.py suggest-prompt TEXT                  the prompt that asks the chair to pick seats
  coach.py suggest-pick TEXT < REPLY            persona ids (comma list, may be empty); why on stderr
  coach.py attach-read SRC OUTDIR               read a file, PDF or link: OUTDIR/{name,kind,text}
  coach.py summary-prompt NAME TEXTFILE         the prompt that summarizes one long source
  coach.py attach-share OUTDIR [SUMMARYFILE]    decide full, summary or start; OUTDIR/{shared,out}
  coach.py block DIR...                         the ATTACHMENTS block every seat reads
"""
import os, re, subprocess, sys, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

MAX_QUESTIONS = 3
MAX_SUGGESTED = 4
MAX_BYTES = 20 * 1024 * 1024
SHARE_LIMIT = 12000
SUMMARY_INPUT = 150000

FIELDS = [("GOAL", "Goal"), ("CONTEXT", "Context"), ("CONSTRAINTS", "Constraints"), ("GOOD ANSWER", "A good answer")]

STOP = set("""about after again also because before being best could does doing from good have help into just
like make more most need only other over really right should some than that their them then
there these they think this those very want were what when where which while will with would
your ours yours answer goal context constraints none given looks""".split())


def one_line(s):
    return " ".join(s.split())


def questions_prompt(q):
    return ("You are the CHAIR of an AI council. Before the council answers, help the person sharpen their question.\n\n"
            "THEIR QUESTION:\n%s\n\n--- YOUR JOB ---\nAsk at most %d short clarifying questions whose answers would change the advice most: "
            "the goal, the situation, the limits, or what a good answer looks like. Each question is one sentence of at most 20 words. "
            "Reply with only the questions, one per line, each starting with \"Q: \". If the question is already clear, reply with exactly \"NONE\".\n"
            % (q.strip(), MAX_QUESTIONS))


def parse_questions(reply):
    out = []
    for line in reply.splitlines():
        t = line.strip().lstrip("-*#").strip()
        m = re.match(r"^(\d+)[.)](.*)$", t)
        if m:
            t = m.group(2).strip()
        up = t.upper()
        if up.startswith("Q:"):
            q = t[2:].strip()
        elif up.startswith("QUESTION:"):
            q = t[9:].strip()
        elif t.endswith("?") and not up.startswith("NONE"):
            q = t
        else:
            continue
        q = q.strip("*").strip()
        if q and q not in out:
            out.append(q)
        if len(out) == MAX_QUESTIONS:
            break
    return out


def brief_prompt(q, qa):
    p = "You are the CHAIR of an AI council. Rewrite the person's question into a brief the council will read.\n\nTHEIR QUESTION:\n%s\n" % q.strip()
    answered = [(x, a) for x, a in qa if a.strip()]
    if answered:
        p += "\nTHEIR ANSWERS TO YOUR CLARIFYING QUESTIONS:\n"
        for x, a in answered:
            p += "Q: %s\nA: %s\n" % (one_line(x), one_line(a))
    p += ("\n--- YOUR JOB ---\nReply with exactly four lines and nothing else:\n"
          "GOAL: <what they want to decide or achieve, in one sentence>\n"
          "CONTEXT: <the facts that matter, in one or two sentences>\n"
          "CONSTRAINTS: <limits on money, time, people or risk, or \"None given\">\n"
          "GOOD ANSWER: <what a useful answer looks like: its form and what it must cover>\n"
          "Use only what they said. Do not invent facts.\n")
    return p


def parse_brief(reply, q):
    b = ["", "", "", ""]
    cur = None
    for line in reply.splitlines():
        t = line.strip().lstrip("-*").strip().replace("**", "").strip()
        up = t.upper()
        hit = None
        for i, (key, label) in enumerate(FIELDS):
            keys = [key, label.upper()] + (["WHAT A GOOD ANSWER LOOKS LIKE"] if i == 3 else [])
            for k in keys:
                if up.startswith(k + ":"):
                    hit = (i, t[len(k) + 1:].strip())
                    break
            if hit:
                break
        if hit:
            b[hit[0]] = hit[1]
            cur = hit[0]
        elif t and cur is not None:
            b[cur] = (b[cur] + " " + t).strip()
    if not b[0].strip():
        b[0] = one_line(q)
    return b


def brief_markdown(b):
    lines = []
    for (_, label), v in zip(FIELDS, b):
        v = one_line(v)
        if v:
            lines.append("- **%s:** %s" % (label, v))
    return "\n".join(lines)


# ---- The suggested panel ----

def words(s):
    low = "".join(c.lower() if c.isascii() else c for c in s)
    out = []
    for w in re.split(r"[^A-Za-z0-9]", "".join(c if (c.isascii() and c.isalnum()) else " " for c in low)):
        if len(w) >= 4 and w not in STOP and w not in out:
            out.append(w)
    return out


def library_personas():
    import personas
    seen, out = set(), []
    for x in personas.all_personas(personas.load_library()):
        if x["id"] in seen:
            continue
        seen.add(x["id"])
        out.append(x)
    return out


def suggest_by_words(text, xs):
    q = words(text)
    scored = []
    for p in xs:
        pw = words("%s %s %s" % (p["name"], p.get("expertise", ""), p.get("brief", "")))
        hits = [w for w in q if w in pw]
        if hits:
            scored.append((len(hits), p, hits))
    scored.sort(key=lambda t: -t[0])
    scored = scored[:MAX_SUGGESTED]
    if len(scored) < 2:
        return [], "No persona matched the question closely, so each AI answers as itself."
    matched = []
    for _, _, h in scored:
        for w in h:
            if w not in matched:
                matched.append(w)
    return [p["id"] for _, p, _ in scored], "Matched on: %s." % ", ".join(matched[:6])


def suggest_prompt(text, xs):
    p = "You are the CHAIR of an AI council. Pick the seats for this question.\n\nTHE QUESTION:\n%s\n\nPERSONAS (id: name, expertise):\n" % text.strip()
    for x in xs:
        p += "- %s: %s, %s\n" % (x["id"], x["name"], one_line(x.get("expertise", "")))
    p += ("\n--- YOUR JOB ---\nPick 2 to %d persona ids whose views matter most for this question, and who will disagree usefully. "
          "Reply with exactly two lines:\nSEATS: <ids, comma separated>\nWHY: <one sentence>\n" % MAX_SUGGESTED)
    return p


def parse_suggestion(reply):
    import personas
    packs = personas.load_library()
    ids, why = [], ""
    for line in reply.splitlines():
        t = line.strip().replace("**", "")
        up = t.upper()
        if up.startswith("SEATS:"):
            for x in [s.strip().strip("`.\"") for s in t[6:].split(",")]:
                if not x:
                    continue
                p = personas.find_persona(packs, x)
                if p and p["id"] not in ids:
                    ids.append(p["id"])
        elif up.startswith("WHY:"):
            why = t[4:].strip()
    return ids[:MAX_SUGGESTED], why


# ---- Attachments ----

def tidy(s):
    out, blank = [], 0
    for line in s.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
        l = line.rstrip()
        if not l.strip():
            blank += 1
            if blank > 1:
                continue
        else:
            blank = 0
        out.append(l)
    return "\n".join(out).strip()


def html_to_text(html):
    s = html
    for tag in ("script", "style", "noscript", "svg", "head"):
        while True:
            low = s.lower()
            a = low.find("<" + tag)
            if a < 0:
                break
            end = "</%s>" % tag
            b = low.find(end, a)
            s = s[:a] + " " + (s[b + len(end):] if b >= 0 else "")
    out, in_tag, tag = [], False, ""
    block = {"p", "br", "div", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "section", "article"}
    for c in s:
        if not in_tag and c == "<":
            in_tag, tag = True, ""
        elif in_tag and c == ">":
            in_tag = False
            t = re.split(r"\s", tag.lstrip("/"))[0].lower()
            if t in block:
                out.append("\n")
        elif in_tag:
            tag += c
        else:
            out.append(c)
    t = "".join(out)
    for a, b in (("&nbsp;", " "), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&apos;", "'"), ("&amp;", "&")):
        t = t.replace(a, b)
    return tidy("\n".join(" ".join(l.split()) for l in t.split("\n")))


def pdf_text(path):
    try:
        r = subprocess.run(["pdftotext", "-q", path, "-"], capture_output=True, text=True, timeout=60)
    except FileNotFoundError:
        raise ValueError("Reading a PDF here needs pdftotext (poppler). The desktop app reads PDFs itself.")
    t = tidy(r.stdout or "")
    if r.returncode != 0:
        raise ValueError("Could not read that PDF.")
    if not t:
        raise ValueError("That PDF has no text to read. It may be a scan.")
    return t


def read_source(src, workdir):
    s = src.strip()
    if s.startswith("http://") or s.startswith("https://"):
        try:
            req = urllib.request.Request(s, headers={"User-Agent": "roundtable-cli"})
            with urllib.request.urlopen(req, timeout=60) as r:
                ctype = (r.headers.get("Content-Type") or "").lower()
                data = r.read(MAX_BYTES + 1)
        except urllib.error.HTTPError as e:
            raise ValueError("That link answered with error %d." % e.code)
        except Exception:
            raise ValueError("Could not open that link. Check it and your connection.")
        if len(data) > MAX_BYTES:
            raise ValueError("That link is larger than %d MB." % (MAX_BYTES // 1024 // 1024))
        if "pdf" in ctype or data.startswith(b"%PDF"):
            p = os.path.join(workdir, "link.pdf")
            with open(p, "wb") as f:
                f.write(data)
            text = pdf_text(p)
        elif "html" in ctype or "<html" in data[:512].decode("utf-8", "replace").lower():
            text = html_to_text(data.decode("utf-8", "replace"))
        else:
            if b"\0" in data[:8000]:
                raise ValueError("%s is not a text file or a PDF." % s)
            text = tidy(data.decode("utf-8", "replace"))
        if not text.strip():
            raise ValueError("That link has no text to read.")
        return s, "link", text
    name = os.path.basename(s) or s
    if not os.path.exists(s):
        raise ValueError("Could not find %s." % name)
    if os.path.getsize(s) > MAX_BYTES:
        raise ValueError("%s is larger than %d MB." % (name, MAX_BYTES // 1024 // 1024))
    with open(s, "rb") as f:
        data = f.read()
    if data.startswith(b"%PDF") or name.lower().endswith(".pdf"):
        return name, "pdf", pdf_text(s)
    if b"\0" in data[:8000]:
        raise ValueError("%s is not a text file or a PDF." % name)
    return name, "text", tidy(data.decode("utf-8", "replace"))


def summary_prompt(name, text):
    return ("You are the CHAIR of an AI council. Every councilor will read your summary of this material instead of the whole thing.\n\n"
            "SOURCE: %s\n\n--- MATERIAL ---\n%s\n\n--- YOUR JOB ---\nSummarize it in at most 600 words. Keep every number, name, date, "
            "limit and decision that a councilor would need. Use short bullets. Do not add facts.\n" % (name, text[:SUMMARY_INPUT]))


def read(p):
    with open(p, encoding="utf-8") as f:
        return f.read()


def write(p, s):
    with open(p, "w", encoding="utf-8") as f:
        f.write(s)


def block(dirs):
    out = "Every seat reads the same material.\n"
    for d in dirs:
        shared = read(os.path.join(d, "shared"))
        note = {"summary": " (summary)", "start": " (first %d characters)" % SHARE_LIMIT}.get(shared, "")
        out += "\n### %s%s\n%s\n" % (read(os.path.join(d, "name")), note, read(os.path.join(d, "out")).strip())
    return out.rstrip()


def main(argv):
    cmd = argv[0] if argv else ""
    if cmd == "questions-prompt":
        sys.stdout.write(questions_prompt(argv[1]))
    elif cmd == "parse-questions":
        qs = parse_questions(sys.stdin.read())
        print("\n".join("Q: " + q for q in qs) if qs else "NONE")
    elif cmd == "brief-prompt":
        qa = []
        if len(argv) > 2 and os.path.exists(argv[2]):
            for line in read(argv[2]).splitlines():
                if line.strip():
                    q, _, a = line.partition("\t")
                    qa.append((q, a))
        sys.stdout.write(brief_prompt(argv[1], qa))
    elif cmd == "parse-brief":
        sys.stdout.write(brief_markdown(parse_brief(sys.stdin.read(), argv[1])))
    elif cmd == "suggest-prompt":
        sys.stdout.write(suggest_prompt(argv[1], library_personas()))
    elif cmd == "suggest-pick":
        reply = sys.stdin.read()
        ids, why = parse_suggestion(reply) if reply.strip() else ([], "")
        if not ids:
            ids, why = suggest_by_words(argv[1], library_personas())
        sys.stderr.write("NOTE: suggested seats: %s. %s\n" % (", ".join(ids) if ids else "generic", why))
        print(",".join(ids))
    elif cmd == "attach-read":
        try:
            name, kind, text = read_source(argv[1], argv[2])
        except ValueError as e:
            sys.stderr.write("ERROR: %s\n" % e)
            return 2
        write(os.path.join(argv[2], "name"), name)
        write(os.path.join(argv[2], "kind"), kind)
        write(os.path.join(argv[2], "text"), text)
        print("long" if len(text) > SHARE_LIMIT else "short")
    elif cmd == "summary-prompt":
        sys.stdout.write(summary_prompt(argv[1], read(argv[2])))
    elif cmd == "attach-share":
        d = argv[1]
        text = read(os.path.join(d, "text"))
        summary = read(argv[2]).strip() if len(argv) > 2 and os.path.exists(argv[2]) else ""
        if len(text) <= SHARE_LIMIT:
            shared, out = "full", text
        elif summary:
            shared, out = "summary", summary
        else:
            shared, out = "start", text[:SHARE_LIMIT]
        write(os.path.join(d, "shared"), shared)
        write(os.path.join(d, "out"), out)
        sys.stderr.write("NOTE: attached %s (%d characters, shared %s).\n" % (read(os.path.join(d, "name")), len(text), shared))
    elif cmd == "block":
        sys.stdout.write(block(argv[1:]))
    else:
        sys.stderr.write(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
