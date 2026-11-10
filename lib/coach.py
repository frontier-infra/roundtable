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
  coach.py clarify-prompt QUESTION              the prompt that checks for missing key facts
  coach.py clarify-questions < REPLY            the open questions (two or three), or nothing
  coach.py open-section QFILE                   the OPEN QUESTIONS text every seat reads
  coach.py chair-prompt QUESTION ROUND POSFILE  the chair's prompt after a round
  coach.py deliberation-prompt BASEFILE PREV POSFILE ROUND CHAIRFILE   a round 2+ prompt
  coach.py agreed < CHAIR                       exit 0 on true agreement (no listed differences)
  coach.py verdict-prompt SHAPE QUESTION BRIEFFILE QFILE POSFILE [MATFILE] [MODE] the verdict prompt
  coach.py seat-rule MODE                       the LENGTH section at the end of every seat's prompt
  coach.py estimate MODE ROUNDS                 "Deep Think: about N minutes."
  coach.py deep-hint < REPLY                    the coach's Deep Think suggestion as a NOTE line, or nothing
  coach.py codex-config CONFIG.TOML             the part of a Codex config a seat keeps
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


MIN_CLARIFY = 2


def clarify_prompt(q):
    """Same text as coach::clarify_prompt in the Rust engine."""
    return ("You are the CHAIR of an AI council. Before the council answers, decide whether the person must answer questions first.\n\n"
            "THEIR QUESTION:\n%s\n\n--- YOUR JOB ---\nMost questions give the situation the choice turns on (numbers, terms, a team, a deadline), "
            "so advisers can pick and state assumptions for the rest. For those, reply with exactly \"NONE\". Ask only when the question names options "
            "but gives none of the facts the choice turns on, such as the workload, revenue, growth, size or goal, so that the right pick changes with the "
            "answers. Then ask the two or three questions whose answers would change the pick most, one per line, each starting with \"Q: \", each one "
            "sentence of at most 20 words.\n" % q.strip())


def clarify_questions(reply):
    qs = parse_questions(reply)
    return qs if len(qs) >= MIN_CLARIFY else []


# ---- Deliberation and the verdict (same text as council.rs and verdict.rs) ----

def chair_prompt(question, rnd, positions):
    return ("You are the neutral CHAIR of an AI design council. The councilors submitted these round %d positions to this question:\n\n%s\n\n--- POSITIONS ---\n\n%s"
            "--- YOUR JOB ---\nLine 1 must be EXACTLY one of: \"VERDICT: CONSENSUS\" or \"VERDICT: CONTINUE\". Line 2: \"LEADING PICK: <the option most seats back, in one sentence>\". "
            "Then give: **AGREED** (bullets) and **REMAINING DISAGREEMENTS** (bullets; name who holds what). Then **DIFFERENCES**: one bullet for each place the seats give "
            "a different number (a price, cost, size, date or percentage), a different threshold or decision rule, or a different first step, naming who said what; "
            "write \"- none\" when there are none. CONSENSUS only on true agreement: the same pick, the same numbers and thresholds (within rounding), and the same "
            "first step. Any bullet under DIFFERENCES means CONTINUE.\n" % (rnd, question, positions))


def deliberation_prompt(base, prev, positions, rnd, chair):
    p = "%s\n\n---\n## COUNCIL DELIBERATION: every councilor's position from round %d:\n\n%s" % (base, prev, positions)
    if chair.strip():
        p += "## THE CHAIR'S READING OF ROUND %d\n%s\n\n" % (prev, chair.strip())
    p += ("## YOUR TASK (round %d)\nYou are ONE councilor. You have now read everyone. First argue against the leading pick: under \"CASE AGAINST:\", "
          "give the strongest case that it is wrong for this person, in at most three bullets, even if you back it. Then settle every difference in numbers, "
          "thresholds or first steps: give your figure or step and why. Then update your position. State explicitly: (a) where a peer changed your mind; name them; "
          "(b) where you still disagree and why; (c) your sharpened current answer, under \"POSITION:\". Begin your reply with \"CASE AGAINST:\".\n" % rnd)
    return p


def is_consensus(chair):
    first = (chair.splitlines() or [""])[0].lower()
    i = first.find("verdict:")
    return i >= 0 and first[i + len("verdict:"):].lstrip().startswith("consensus")


def _is_none(s):
    s = s.strip().strip('."()_*').lower()
    return s in ("none", "no differences", "nothing")


def differences(chair):
    out, inside = [], False
    for line in chair.splitlines():
        t = line.strip().replace("**", "").strip()
        if t.upper().startswith("DIFFERENCES"):
            inside = True
            rest = t[len("DIFFERENCES"):].lstrip(":").strip()
            if rest and not _is_none(rest):
                out.append(rest)
            continue
        if not inside:
            continue
        if t.startswith("- ") or t.startswith("* "):
            if not _is_none(t[2:]):
                out.append(t[2:].strip())
        elif t:
            inside = False
    return out


def agreed(chair):
    return is_consensus(chair) and not differences(chair)


# ---- Modes: Quick (the default) and Deep Think. Same rules as desktop/engine/src/mode.rs ----

MODES = {"": "quick", "quick": "quick", "fast": "quick", "deep": "deep", "deep-think": "deep", "deepthink": "deep", "think": "deep"}
MODE_TITLE = {"quick": "Quick", "deep": "Deep Think"}
MODE_ROUNDS = {"quick": 1, "deep": 4}
SEAT_WORDS = {"quick": 250, "deep": 800}


def mode_of(s):
    return MODES.get((s or "").strip().lower().replace("_", "-").replace(" ", "-"))


def seat_rule(mode):
    if mode == "deep":
        return ("## LENGTH\nTake up to about %d words. Lead with your pick and how sure you are. Then reason it through: what each serious option "
                "costs and buys, the risks that could hurt this person, the facts you do not have and how to find them out, and the steps in order." % SEAT_WORDS[mode])
    return ("## LENGTH\nKeep your answer to about %d words. Lead with your pick and how sure you are. Then give the main trade-off, "
            "the risks that could hurt this person, and the facts you do not have that would change the pick. "
            "Include CASE AGAINST: two short bullets, the strongest case that your pick is wrong for this person." % SEAT_WORDS["quick"])


def estimate_minutes(mode, rounds):
    rounds = max(1, min(4, rounds))
    seat, verdict, coach = (140, 75, 45) if mode == "deep" else (50, 30, 0)
    chair = rounds * 30 if rounds > 1 and mode == "deep" else 0
    secs = coach + rounds * seat + chair + verdict
    return max(1, -(-secs // 60))


def estimate_line(mode, rounds):
    n = estimate_minutes(mode, rounds)
    return "%s: about %d minute%s." % (MODE_TITLE[mode], n, "" if n == 1 else "s")


SHAPE_JOB = {
    "decision": "Then **TRADE-OFFS**: one short bullet per serious option (at most three), what it costs and what it buys. Then **RISKS**: at most three short bullets, the risks that could hurt this person, each with how to blunt it. Then **UNKNOWNS**: at most two short bullets, the facts that would change the pick and how to find each one out.",
    "pros-cons": "Then **PROS** and **CONS** of the pick: at most two short bullets each.",
    "plan": "Then **PLAN**: at most three more numbered steps (4 to 6), each with who and by when.",
    "vote": "Then **VOTES**: one bullet per seat, \"<seat name>: <the option it backs>\". Then \"RESULT: <option> wins <n> to <m>\", or \"RESULT: tie\".",
}
DEEP_JOB = {
    "decision": "",
    "plan": "",
    "pros-cons": "**PROS** and **CONS**: the case for and against the pick, in bullets.\n",
    "vote": "**VOTES**: one bullet per seat, \"<seat name>: <the option it backs>\". Then \"RESULT: <option> wins <n> to <m>\", or \"RESULT: tie\".\n",
}
VERDICT_DEEP = ("Then write the full verdict under these bold headings, in this order:\n"
                "**REASONING**: why the pick holds, grouped by theme; give each theme a short bold label and name the seats behind it.\n"
                "**TRADE-OFFS**: what each serious option costs and buys.\n"
                "**RISKS**: what could hurt this person, how likely it is, and how to blunt each one.\n"
                "**UNKNOWNS**: the facts nobody has, why each one matters, and how to find it out (who, how, by when).\n"
                "**PLAN**: every step in order, numbered, each with who and by when, and the checkpoint that would make the person stop or switch.\n")
VERDICT_DEEP_END = ("**THE MINORITY VIEW IN FULL**: the strongest case against the pick, argued fairly on its own terms, and what would have to be true for it to win.\n"
                    "Use the seats' own numbers and add no new facts. Take the room this needs, up to about 1,200 words. The decision lines come first: the person reads them first.\n")
VERDICT_ENDING = ("Then end with exactly these two lines:\nMINORITY VIEW: <the strongest case against the pick, in one or two sentences: what it says and when it would be the right call. "
                  "Never \"None\": when every seat agreed, give the best case for the strongest other option>\n"
                  "HELD BY: <the seats that held it, by their names above, or \"Nobody at the table\">\n")
VERDICT_CORE = ("Lead with one decision. Start with exactly these lines:\nDECISION: <the pick, in one sentence>\nCONFIDENCE: <high, medium or low> (<0 to 100>%)\n"
                "WOULD CHANGE IT: <the fact or event that would change the pick, in one sentence>\nNEXT STEPS:\n1. <the first concrete step, one the person can start today>\n"
                "2. <the second step>\n3. <the third step>\n")
VERDICT_CLARIFY = ("Key facts are missing (the OPEN QUESTIONS above), so open with the questions, not the pick. Start with exactly this line, then two or three "
                   "numbered questions (sharpen the open questions if that helps):\nQUESTIONS FIRST:\n1. <question>\n2. <question>\nThen give the decision lines below. "
                   "DECISION names the pick that holds under the most likely answers and says how the answers would change it (\"If ..., then ...\").\n")


def verdict_prompt(shape, question, brief, open_qs, positions, material="", mode="quick"):
    p = "You are the neutral CHAIR of an AI council. Write the final verdict on this question:\n\n%s\n\n" % question.strip()
    if brief.strip():
        p += "THE BRIEF:\n%s\n\n" % brief.strip()
    # The chair reads the same files and links as every seat, or it doubts what they quote.
    if material.strip():
        p += "THE MATERIAL EVERY SEAT READ:\n%s\n\n" % material.strip()
    if open_qs:
        p += "OPEN QUESTIONS:\n" + "".join("- %s\n" % q for q in open_qs) + "\n"
    p += "--- FINAL POSITIONS ---\n\n" + positions + "--- YOUR JOB ---\n"
    if open_qs:
        p += VERDICT_CLARIFY
    p += VERDICT_CORE
    if mode == "deep":
        p += VERDICT_DEEP + DEEP_JOB[shape] + VERDICT_DEEP_END
    else:
        p += SHAPE_JOB[shape] + "\nUse the seats' own numbers and add no new facts. Keep the decision lines under 200 words and the whole verdict under 300: the person reads this first, and the seat answers sit folded under it.\n"
    return p + VERDICT_ENDING


def open_questions_section(qs):
    return "The chair would ask the person these first. They are not answered yet, so say how each answer would change your pick.\n" + "\n".join("- " + q for q in qs)


# ---- Seat isolation: the part of a Codex config.toml a seat keeps (same rule as clis.rs) ----

CODEX_KEEP = ("model", "model_provider", "model_reasoning_effort", "model_reasoning_summary", "model_verbosity", "preferred_auth_method",
              "forced_login_method", "forced_chatgpt_workspace_id", "cli_auth_credentials_store", "openai_base_url", "chatgpt_base_url")


def filter_codex_config(text):
    out, table = [], None
    for line in text.splitlines():
        t = line.strip()
        if t.startswith("["):
            name = t.lstrip("[").split("]")[0].strip()
            keep = name == "model_providers" or name.startswith("model_providers.")
            if keep:
                out.append(line)
            table = name if keep else ""
            continue
        if table is None:
            key = t.split("=")[0].strip()
            if "=" in t and key in CODEX_KEEP:
                out.append(line)
        elif table:
            out.append(line)
    return "\n".join(out) + "\n"


def brief_prompt(q, qa):
    p = "You are the CHAIR of an AI council. Rewrite the person's question into a brief the council will read.\n\nTHEIR QUESTION:\n%s\n" % q.strip()
    answered = [(x, a) for x, a in qa if a.strip()]
    if answered:
        p += "\nTHEIR ANSWERS TO YOUR CLARIFYING QUESTIONS:\n"
        for x, a in answered:
            p += "Q: %s\nA: %s\n" % (one_line(x), one_line(a))
    p += ("\n--- YOUR JOB ---\nReply with exactly five lines and nothing else:\n"
          "GOAL: <what they want to decide or achieve, in one sentence>\n"
          "CONTEXT: <the facts that matter, in one or two sentences>\n"
          "CONSTRAINTS: <limits on money, time, people or risk, or \"None given\">\n"
          "GOOD ANSWER: <what a useful answer looks like: its form and what it must cover>\n"
          + DEEP_LINE + "Use only what they said. Do not invent facts.\n")
    return p


# The brief's last line: should this question get Deep Think? Same words as coach.rs.
DEEP_LINE = ("DEEP THINK: <yes or no>: <why, in one short sentence>. Say yes when the decision is complex: several stakeholders, "
             "money or legal stakes, more than three options, or missing facts the pick depends on.\n")


def _deep_rest(t):
    up = t.upper()
    for k in ("DEEP THINK:", "DEEP-THINK:", "DEEPTHINK:"):
        if up.startswith(k):
            return t[len(k):].strip()
    return None


def parse_deep(reply):
    """The coach's Deep Think line: (suggested, why). No line means no suggestion."""
    for line in reply.splitlines():
        t = line.strip().lstrip("-*").strip().replace("**", "").strip()
        rest = _deep_rest(t)
        if rest is None:
            continue
        low = rest.lower()
        yes = low.startswith("yes")
        why = rest[3 if yes else 2:] if (yes or low.startswith("no")) else rest
        why = why.lstrip(":,-. ").strip()
        return yes, (why if yes else "")
    return False, ""


def deep_hint(why):
    why = why.strip().rstrip(".")
    if not why:
        return "NOTE: The coach suggests Deep Think for this one. Run it again with --mode deep."
    return "NOTE: The coach suggests Deep Think: %s. Run it again with --mode deep." % why


def parse_brief(reply, q):
    b = ["", "", "", ""]
    cur = None
    for line in reply.splitlines():
        t = line.strip().lstrip("-*").strip().replace("**", "").strip()
        if _deep_rest(t) is not None:
            cur = None
            continue
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
    elif cmd == "clarify-prompt":
        sys.stdout.write(clarify_prompt(argv[1]))
    elif cmd == "clarify-questions":
        qs = clarify_questions(sys.stdin.read())
        if qs:
            print("\n".join(qs))
    elif cmd == "open-section":
        qs = [l for l in read(argv[1]).splitlines() if l.strip()]
        if qs:
            sys.stdout.write(open_questions_section(qs))
    elif cmd == "chair-prompt":
        sys.stdout.write(chair_prompt(argv[1], int(argv[2]), read(argv[3])))
    elif cmd == "deliberation-prompt":
        sys.stdout.write(deliberation_prompt(read(argv[1]), int(argv[2]), read(argv[3]), int(argv[4]), read(argv[5]) if os.path.exists(argv[5]) else ""))
    elif cmd == "agreed":
        return 0 if agreed(sys.stdin.read()) else 1
    elif cmd == "verdict-prompt":
        qs = [l for l in read(argv[4]).splitlines() if l.strip()] if os.path.exists(argv[4]) else []
        sys.stdout.write(verdict_prompt(argv[1], argv[2], read(argv[3]) if os.path.exists(argv[3]) else "", qs, read(argv[5]),
                                        read(argv[6]) if len(argv) > 6 and os.path.exists(argv[6]) else "",
                                        mode_of(argv[7]) or "quick" if len(argv) > 7 else "quick"))
    elif cmd == "seat-rule":
        sys.stdout.write(seat_rule(mode_of(argv[1]) or "quick"))
    elif cmd == "estimate":
        sys.stdout.write(estimate_line(mode_of(argv[1]) or "quick", int(argv[2])))
    elif cmd == "mode":
        m = mode_of(argv[1])
        if not m:
            sys.stderr.write("ERROR: Unknown mode '%s'. Use quick or deep.\n" % argv[1].strip().lower())
            return 2
        sys.stdout.write(m)
    elif cmd == "deep-hint":
        yes, why = parse_deep(sys.stdin.read())
        if yes:
            print(deep_hint(why))
    elif cmd == "codex-config":
        sys.stdout.write(filter_codex_config(read(argv[1]) if os.path.exists(argv[1]) else ""))
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
