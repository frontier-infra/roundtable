#!/usr/bin/env python3
"""Roundtable usefulness bench.

Each scenario goes to the Roundtable council and to one single AI. A judge
on a different model scores both answers on the rubric (1 to 5 per line) and
says which one it would rather act on. The judge runs twice per scenario with
the answer order swapped, to cancel position bias.

Writes bench/results/<date>/<id>.json (both answers, the transcript, the
judge replies) and bench/results/<date>.md (scores and totals).

Every model call goes through the roundtable command itself (--heads X), so
it uses the same signed-in apps and keys as a person would.

Standard library only. Python 3.9 or newer.
"""
import argparse
import concurrent.futures as cf
import datetime as dt
import json
import os
import random
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

RUBRIC = [
    ("risks", "Surfaces the key risks", "Names the risks that could actually hurt this person, specific to their situation, not generic warnings."),
    ("tradeoffs", "Names trade-offs", "States what each option costs and buys, so the person can see what they give up."),
    ("recommendation", "Clear recommendation with confidence", "Makes one call the reader can find quickly, says how sure it is and what would change it. Several advisers that each give their own call, with nobody reconciling them, is not one clear call. For an 'it depends' question, the call is what to find out first and the rule that decides once known."),
    ("unknowns", "Flags what is unknown", "Says which facts it does not have and why they matter, instead of assuming them."),
    ("hidden_facts", "Catches the hidden facts", "Raises the hidden facts listed below, by warning about them or asking for them. 5 means nearly all; 1 means none."),
    ("next_steps", "Actionable next steps", "Gives concrete, ordered steps the person can start today."),
    ("minority_view", "Honest minority view", "Presents the strongest case against its own recommendation, or a credible dissenting option, fairly."),
]
KEYS = [k for k, _, _ in RUBRIC]


def load_scenarios(path):
    with open(path) as f:
        data = json.load(f)
    sc = data["scenarios"]
    ids = set()
    for s in sc:
        for k in ("id", "topic", "shape", "question", "hidden_facts"):
            assert k in s, f"{s.get('id')}: missing {k}"
        assert s["shape"] in ("recommend", "depends"), s["id"]
        assert len(s["hidden_facts"]) >= 3, s["id"]
        assert s["id"] not in ids, s["id"]
        ids.add(s["id"])
    return sc


MODES = ("quick", "deep", "none")
# Rounds a mode runs when --rounds is not given ("none": a build before modes, one round).
MODE_ROUNDS = {"quick": 1, "deep": 4, "none": 1}


def mode_label(mode):
    return {"quick": "Quick", "deep": "Deep Think", "none": "No mode (a build before modes)"}.get(mode or "none", str(mode))


def ask(rt, question, heads, rounds, timeout, mode="none"):
    """Run `roundtable ask` with --events. Returns a dict with per-round head
    answers, chair verdicts, and the transcript. `mode` quick or deep adds
    --mode; "none" leaves it off (a build before modes). `rounds` None leaves
    --rounds off, so the mode picks."""
    cmd = [rt, "ask", "-q", question, "--heads", heads]
    if mode in ("quick", "deep"):
        cmd += ["--mode", mode]
    if rounds is not None:
        cmd += ["--rounds", str(rounds)]
    cmd += ["--timeout", str(timeout), "--events", "--no-history"]
    n = rounds if rounds is not None else MODE_ROUNDS.get(mode, 1)
    t0 = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True,
                       timeout=timeout * (n * 3 + 6) + 60)
    out = {"cmd": cmd[:2] + ["-q", "<question>"] + cmd[4:], "exit": p.returncode, "mode": mode,
           "secs": round(time.time() - t0, 1), "heads": {}, "chair": {},
           "transcript": "", "consensus": None, "verdict": "", "clarify": [],
           "stderr_tail": p.stderr[-400:]}
    for line in p.stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        kind = ev.get("type") or ev.get("event") or next(iter(ev), "")
        body = ev
        if len(ev) == 1 and isinstance(ev[next(iter(ev))], dict):
            kind, body = next(iter(ev.items()))
        kind = str(kind).lower().replace("_", "")
        if kind == "headdone":
            r = str(body.get("round"))
            out["heads"].setdefault(r, {})[body["head"]] = {
                "status": body.get("status"), "text": body.get("text", ""),
                "via": body.get("via"), "message": body.get("message")}
        elif kind == "chairverdict":
            out["chair"][str(body.get("round"))] = body.get("text", "")
        elif kind == "verdict":
            out["verdict"] = body.get("text", "")
        elif kind == "clarify":
            out["clarify"] = body.get("questions", [])
        elif kind == "rundone":
            out["transcript"] = body.get("transcript", "")
            out["consensus"] = body.get("consensus")
    if not out["transcript"] and not out["heads"]:
        out["transcript"] = p.stdout  # a CLI without --events: keep what it printed
    return out


def answered(rec, rnd):
    return {h: v["text"] for h, v in rec["heads"].get(rnd, {}).items()
            if v.get("status") in ("answered", "ok", "done") and v.get("text", "").strip()}


def table_answer(rec):
    """What a person reads at the end of a Roundtable run. With a verdict (every
    table since rc8): the verdict first, then the last round's seat answers, which
    the app shows folded under it. Without one: the last round's positions and the
    chair's last ruling."""
    if not rec["heads"]:
        return rec["transcript"].strip()
    last = max(rec["heads"], key=int)
    if rec.get("verdict", "").strip():
        seats = "\n\n".join(f"### Seat: {h}\n{txt.strip()}" for h, txt in answered(rec, last).items())
        return (f"## Verdict (shown first)\n{rec['verdict'].strip()}\n\n"
                f"## Seat answers (folded under the verdict; opened only to dig deeper)\n\n{seats}")
    parts = []
    for h, txt in answered(rec, last).items():
        parts.append(f"### Councilor: {h}\n{txt.strip()}")
    ch = rec["chair"].get(last) or (rec["chair"][max(rec["chair"], key=int)] if rec["chair"] else "")
    if ch:
        parts.append(f"### Chair ruling (round {last})\n{ch.strip()}")
    return "\n\n".join(parts)


def single_answer(rec):
    a = answered(rec, "1")
    return next(iter(a.values()), "").strip()


def judge_prompt(s, first, second):
    facts = "\n".join(f"{i + 1}. {f}" for i, f in enumerate(s["hidden_facts"]))
    rub = "\n".join(f"- {k}: {name}. {desc}" for k, name, desc in RUBRIC)
    shape = ("The right answer here is a clear recommendation." if s["shape"] == "recommend" else
             "The right answer here is 'it depends': name what to find out first, then the rule that decides.")
    keys = ", ".join(f'"{k}": n' for k in KEYS)
    return f"""You are a strict, fair judge of decision advice. A small-business owner or technical lead typed the question below. Two advisers answered. Score each answer on its own merits, then say which one the person would be better off acting on.

Do not reward length, formatting, or confident tone. A short answer that catches what matters beats a long one that does not. Ignore which adviser is which; you are not told.

## QUESTION
{s['question']}

## WHAT A GOOD ANSWER SURFACES (hidden from the advisers)
{shape}
{facts}

## RUBRIC (each line scored 1 to 5)
Use the whole scale. 1 missing, 2 weak, 3 adequate, 4 good, 5 nothing important missing and an expert would not improve it. Most real answers land at 3 or 4 on most lines.
{rub}

## ANSWER A
{first}

## ANSWER B
{second}

## OUTPUT
Reply with ONLY one JSON object, no prose and no code fence, in exactly this shape:
{{"A": {{{keys}, "facts_caught": [numbers of the hidden facts A raised]}}, "B": {{{keys}, "facts_caught": [...]}}, "prefer": "A" or "B" or "tie", "why": "one sentence"}}"""


def parse_json(text):
    m = re.search(r"\{.*\}", text or "", re.S)
    if not m:
        return None
    blob = m.group(0)
    for cand in (blob, re.sub(r",\s*([}\]])", r"\1", blob)):
        try:
            return json.loads(cand)
        except ValueError:
            pass
    return None


def judge(rt, judge_head, s, rt_ans, single_ans, rt_first, timeout):
    a, b = (rt_ans, single_ans) if rt_first else (single_ans, rt_ans)
    rec = ask(rt, judge_prompt(s, a, b), judge_head, 1, timeout)
    raw = single_answer(rec)
    j = parse_json(raw)
    res = {"rt_first": rt_first, "raw": raw, "secs": rec["secs"], "ok": False}
    if not j or "A" not in j or "B" not in j:
        return res
    rt_key, si_key = ("A", "B") if rt_first else ("B", "A")

    def clean(d):
        sc = {}
        for k in KEYS:
            try:
                sc[k] = max(1, min(5, int(round(float(d.get(k))))))
            except (TypeError, ValueError):
                sc[k] = None
        sc["facts_caught"] = sorted({int(x) for x in d.get("facts_caught", []) if str(x).isdigit()})
        return sc

    pref = str(j.get("prefer", "tie")).strip().upper()
    res.update(ok=True, rt=clean(j[rt_key]), single=clean(j[si_key]), why=j.get("why", ""),
               prefer="rt" if pref == rt_key else "single" if pref == si_key else "tie")
    return res


def result_path(outdir, sid, mode):
    """Quick (and a run before modes) keeps <id>.json; Deep Think keeps <id>.deep.json."""
    return os.path.join(outdir, f"{sid}.json" if mode in ("quick", "none") else f"{sid}.{mode}.json")


def run_scenario(args, s, outdir):
    path = result_path(outdir, s["id"], args.mode)
    if args.resume and os.path.exists(path):
        with open(path) as f:
            return json.load(f)
    # Deep Think is judged against the same single answer as Quick, when Quick already ran.
    other = result_path(outdir, s["id"], "quick")
    single = None
    if args.mode == "deep" and os.path.exists(other):
        with open(other) as f:
            single = json.load(f).get("single")
    log(f"{s['id']}: table ({mode_label(args.mode)})" + ("" if single else " and single"))
    with cf.ThreadPoolExecutor(2) as ex:
        ft = ex.submit(ask, args.rt, s["question"], args.seats, args.rounds, args.timeout, args.mode)
        fs = None if single else ex.submit(ask, args.rt, s["question"], args.single, 1, args.timeout)
        table = ft.result()
        single = single or fs.result()
    rt_ans, si_ans = table_answer(table), single_answer(single)
    rec = {"id": s["id"], "mode": args.mode, "table": table, "single": single,
           "table_answer": rt_ans, "single_answer": si_ans, "judges": []}
    if rt_ans and si_ans:
        rng = random.Random(s["id"])
        first = rng.random() < 0.5
        for order in (first, not first):
            log(f"{s['id']}: judge ({'table' if order else 'single'} shown first)")
            rec["judges"].append(judge(args.rt, args.judge, s, rt_ans, si_ans, order, args.timeout))
    with open(path, "w") as f:
        json.dump(rec, f, indent=1)
    log(f"{s['id']}: done")
    return rec


def mean(xs):
    xs = [x for x in xs if x is not None]
    return round(sum(xs) / len(xs), 2) if xs else None


def summarize(s, rec):
    js = [j for j in rec["judges"] if j.get("ok")]
    row = {"id": s["id"], "topic": s["topic"], "shape": s["shape"], "n_judge": len(js),
           "n_facts": len(s["hidden_facts"]), "mode": rec.get("mode") or rec["table"].get("mode") or "none"}
    for side in ("rt", "single"):
        row[side] = {k: mean([j[side][k] for j in js]) for k in KEYS}
        row[side]["total"] = round(sum(v for v in row[side].values() if v is not None), 2) if js else None
        row[side]["facts"] = mean([len(j[side]["facts_caught"]) for j in js])
    votes = [j["prefer"] for j in js]
    w, l = votes.count("rt"), votes.count("single")
    row["votes"] = votes
    row["pref"] = "table" if w > l else "single" if l > w else "tie" if js else "n/a"
    row["why"] = [j.get("why", "") for j in js]
    t = rec["table"]
    last = max(t["heads"], key=int) if t["heads"] else "1"
    row["rounds_used"] = len(t["heads"])
    row["consensus"] = t.get("consensus")
    row["clarify"] = len(t.get("clarify") or [])
    row["verdict"] = bool((t.get("verdict") or "").strip())
    row["seated"] = {r: sorted(answered(t, r)) for r in t["heads"]}
    row["failed"] = {r: {h: (v.get("message") or v.get("status")) for h, v in hs.items()
                         if h not in answered(t, r)} for r, hs in t["heads"].items()}
    row["secs"] = {"table": t["secs"], "single": rec["single"]["secs"]}
    row["words"] = {"table": len(rec["table_answer"].split()), "single": len(rec["single_answer"].split())}
    return row


def fmt(x):
    return "-" if x is None else (f"{x:.1f}" if isinstance(x, float) else str(x))


def command_line(args, mode):
    base = f"{os.path.basename(args.rt)} ask --heads {args.seats}"
    if mode in ("quick", "deep"):
        base += f" --mode {mode}"
    if args.rounds is not None:
        base += f" --rounds {args.rounds}"
    elif mode == "none":
        base += " --rounds 1"
    return base


def report(args, scen, rows, date):
    short = {"risks": "Risk", "tradeoffs": "Trade", "recommendation": "Rec", "unknowns": "Unkn",
             "hidden_facts": "Facts", "next_steps": "Next", "minority_view": "Minor"}
    order = ("quick", "none", "deep")
    modes = [m for m in order if any(r["mode"] == m for r in rows)]
    # The totals are for the main mode: Quick when it ran, else the one that did.
    main = modes[0] if modes else "none"
    mrows = [r for r in rows if r["mode"] == main]
    L = []
    L.append(f"# Usefulness bench, {date}")
    L.append("")
    L.append(f"- Build: `{args.build}`")
    for m in modes:
        n = sum(r["mode"] == m for r in rows)
        L.append(f"- {mode_label(m)}: `{command_line(args, m)}`, {n} scenario{'s' if n != 1 else ''}")
    L.append(f"- Single AI: `{args.single}` (one answer, same question)")
    L.append(f"- Judge: `{args.judge}`, scored twice per scenario with the order swapped")
    L.append(f"- Scenarios: {len(scen)} (bench/scenarios.json)")
    L.append("")
    w = sum(r["pref"] == "table" for r in mrows)
    l = sum(r["pref"] == "single" for r in mrows)
    t = sum(r["pref"] == "tie" for r in mrows)
    na = sum(r["pref"] == "n/a" for r in mrows)
    L.append(f"## Totals ({mode_label(main)})")
    L.append("")
    L.append(f"Pairwise preference: Roundtable {w}, single AI {l}, tie {t}" + (f", not judged {na}" if na else "") + ".")
    L.append("")
    L.append("| Rubric line | Roundtable | Single AI | Gap |")
    L.append("|---|---|---|---|")
    lines = []
    for k, name, _ in RUBRIC:
        a = mean([r["rt"][k] for r in mrows])
        b = mean([r["single"][k] for r in mrows])
        lines.append((k, name, a, b))
        L.append(f"| {name} | {fmt(a)} | {fmt(b)} | {fmt(None if a is None or b is None else round(a - b, 2))} |")
    ta = mean([r["rt"]["total"] for r in mrows])
    tb = mean([r["single"]["total"] for r in mrows])
    L.append(f"| **Total (of 35)** | **{fmt(ta)}** | **{fmt(tb)}** | **{fmt(None if ta is None or tb is None else round(ta - tb, 2))}** |")
    fa = sum((r["rt"]["facts"] or 0) for r in mrows)
    fb = sum((r["single"]["facts"] or 0) for r in mrows)
    nf = sum(r["n_facts"] for r in mrows if r["n_judge"])
    if nf:
        L.append("")
        L.append(f"Hidden facts caught: Roundtable {fa:.1f} of {nf} ({100 * fa / nf:.0f}%), single AI {fb:.1f} of {nf} ({100 * fb / nf:.0f}%).")
    weak = sorted([x for x in lines if x[2] is not None], key=lambda x: x[2])[:3]
    if weak:
        L.append("")
        L.append("Weakest Roundtable lines: " + "; ".join(f"{n} ({fmt(a)})" for _, n, a, _ in weak) + ".")
    secs_s = mean([r["secs"]["single"] for r in mrows])
    ws = mean([r["words"]["single"] for r in mrows])
    L.append("")
    L.append("## Time and words by mode")
    L.append("")
    L.append("| Mode | Scenarios | Roundtable total (of 35) | Mean time | Mean words judged |")
    L.append("|---|---|---|---|---|")
    base = getattr(args, "baseline_rows", None)
    if base:
        L.append(f"| Baseline ({args.baseline}) | {len(base)} | {fmt(mean([r['rt']['total'] for r in base]))} | "
                 f"{fmt(mean([r['secs']['table'] for r in base]))} s | {fmt(mean([r['words']['table'] for r in base]))} |")
    for m in modes:
        rs = [r for r in rows if r["mode"] == m]
        L.append(f"| {mode_label(m)} | {len(rs)} | {fmt(mean([r['rt']['total'] for r in rs]))} | "
                 f"{fmt(mean([r['secs']['table'] for r in rs]))} s | {fmt(mean([r['words']['table'] for r in rs]))} |")
    L.append(f"| Single AI | {len(mrows)} | {fmt(tb)} | {fmt(secs_s)} s | {fmt(ws)} |")
    if base:
        L.append("")
        L.append(f"## Against the baseline ({args.baseline})")
        L.append("")
        bp = {k: sum(r["pref"] == k for r in base) for k in ("table", "single", "tie")}
        L.append(f"Pairwise preference then: Roundtable {bp['table']}, single AI {bp['single']}, tie {bp['tie']}. Now ({mode_label(main)}): Roundtable {w}, single AI {l}, tie {t}.")
        L.append("")
        L.append("| Rubric line | Roundtable before | Roundtable now | Change | Single before | Single now |")
        L.append("|---|---|---|---|---|---|")
        for k, name, _ in RUBRIC + [("total", "**Total (of 35)**", "")]:
            a0 = mean([r["rt"][k] for r in base])
            a1 = mean([r["rt"][k] for r in mrows])
            b0 = mean([r["single"][k] for r in base])
            b1 = mean([r["single"][k] for r in mrows])
            ch = None if a0 is None or a1 is None else round(a1 - a0, 2)
            L.append(f"| {name} | {fmt(a0)} | {fmt(a1)} | {fmt(ch)} | {fmt(b0)} | {fmt(b1)} |")
        L.append("")
        L.append("| Scenario | Roundtable before | Roundtable now | Time before | Time now | Pick before | Pick now |")
        L.append("|---|---|---|---|---|---|---|")
        bb = {r["id"]: r for r in base}
        for r in mrows:
            o = bb.get(r["id"])
            if not o:
                continue
            L.append(f"| {r['id']} | {fmt(o['rt']['total'])} | {fmt(r['rt']['total'])} | {fmt(o['secs']['table'])} s | {fmt(r['secs']['table'])} s | {o['pref']} | {r['pref']} |")
    deep = [r for r in rows if r["mode"] == "deep"]
    if deep and main != "deep":
        L.append("")
        L.append("## Deep Think")
        L.append("")
        L.append(f"| Scenario | {mode_label(main)} total | Deep Think total | Single AI | {mode_label(main)} time | Deep Think time | {mode_label(main)} words | Deep Think words | Pick |")
        L.append("|---|---|---|---|---|---|---|---|---|")
        qq = {r["id"]: r for r in mrows}
        for r in deep:
            q = qq.get(r["id"])
            g = (lambda f: fmt(f(q)) if q else "-")
            L.append(f"| {r['id']} | {g(lambda x: x['rt']['total'])} | {fmt(r['rt']['total'])} | {fmt(r['single']['total'])} | "
                     f"{g(lambda x: x['secs']['table'])} s | {fmt(r['secs']['table'])} s | {g(lambda x: x['words']['table'])} | {fmt(r['words']['table'])} | {r['pref']} |")
    L.append("")
    L.append("## Per scenario")
    L.append("")
    hdr = " | ".join(short[k] for k in KEYS)
    L.append(f"| Scenario | Mode | Side | {hdr} | Total | Facts | Pick |")
    L.append("|---|---|---|" + "---|" * (len(KEYS) + 3))
    for r in sorted(rows, key=lambda r: (order.index(r["mode"]) if r["mode"] in order else 9)):
        for side, label in (("rt", "Roundtable"), ("single", "Single")):
            cells = " | ".join(fmt(r[side][k]) for k in KEYS)
            pick = r["pref"] if side == "rt" else ""
            facts = f"{fmt(r[side]['facts'])}/{r['n_facts']}"
            name = f"{r['id']}{' (depends)' if r['shape'] == 'depends' else ''}" if side == "rt" else ""
            mode = mode_label(r["mode"]) if side == "rt" else ""
            L.append(f"| {name} | {mode} | {label} | {cells} | {fmt(r[side]['total'])} | {facts} | {pick} |")
    L.append("")
    L.append("## Run notes")
    L.append("")
    for r in rows:
        seated = ", ".join(f"r{k}: {'/'.join(v) or 'none'}" for k, v in sorted(r["seated"].items()))
        failed = "; ".join(f"r{k} {h}: {m}" for k, hs in sorted(r["failed"].items()) for h, m in hs.items())
        cons = {True: "consensus", False: "no consensus", None: "no chair ruling"}[r["consensus"]]
        clar = f"; opened with {r['clarify']} questions" if r.get("clarify") else ""
        L.append(f"- **{r['id']}** ({mode_label(r['mode'])}): {r['rounds_used']} round(s), {cons}{clar}; seated {seated}"
                 + (f"; failed {failed}" if failed else "")
                 + f"; {fmt(r['secs']['table'])} s vs {fmt(r['secs']['single'])} s; {r['words']['table']} words; votes {', '.join(r['votes']) or 'none'}.")
        for y in r["why"]:
            if y:
                L.append("  - Judge: " + y.replace(" — ", "; ").replace("—", ", "))
    L.append("")
    L.append("Raw answers, transcripts and judge replies are kept locally in `bench/results/" + date + "/` (not committed).")
    return "\n".join(L) + "\n"


def log(msg):
    print(f"[{dt.datetime.now():%H:%M:%S}] {msg}", file=sys.stderr, flush=True)


def git_build(rt):
    try:
        sha = subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
        br = subprocess.run(["git", "-C", ROOT, "rev-parse", "--abbrev-ref", "HEAD"], capture_output=True, text=True).stdout.strip()
        ver = subprocess.run([rt, "version"], capture_output=True, text=True, timeout=20).stdout.strip().splitlines()
        return f"{br}@{sha}, {ver[0] if ver else 'unknown version'}"
    except Exception as e:  # noqa: BLE001
        return f"unknown ({e})"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--rt", default=os.environ.get("RT_BIN", "roundtable"), help="roundtable command (RT_BIN)")
    ap.add_argument("--seats", default=os.environ.get("RT_SEATS", "claude,codex,grok"))
    ap.add_argument("--mode", default=os.environ.get("RT_MODE", "quick"), choices=MODES,
                    help="quick or deep (Deep Think); none for a build before modes")
    ap.add_argument("--rounds", type=int, default=int(os.environ["RT_ROUNDS"]) if os.environ.get("RT_ROUNDS") else None,
                    help="rounds instead of the mode's (the mode picks when left out)")
    ap.add_argument("--single", default=os.environ.get("RT_SINGLE", "claude"), help="the one AI to compare against")
    ap.add_argument("--judge", default=os.environ.get("RT_JUDGE", "opencode"), help="judge head; must not be a seat")
    ap.add_argument("--timeout", type=int, default=300, help="seconds per model call")
    ap.add_argument("--jobs", type=int, default=3, help="scenarios run at once")
    ap.add_argument("--only", default="", help="comma list of scenario ids")
    ap.add_argument("--date", default=dt.date.today().isoformat())
    ap.add_argument("--scenarios", default=os.path.join(HERE, "scenarios.json"))
    ap.add_argument("--resume", action="store_true", help="reuse saved scenario results for this date")
    ap.add_argument("--report-only", action="store_true", help="rebuild the .md from saved results")
    ap.add_argument("--validate", action="store_true", help="check scenarios.json and the judge prompt, call nothing")
    ap.add_argument("--baseline", default="", help="a past date whose summary.json the report compares against")
    ap.add_argument("--tag", default="", help="a suffix for this run's results name (2026-11-10b)")
    args = ap.parse_args()

    scen = load_scenarios(args.scenarios)
    if args.only:
        want = set(args.only.split(","))
        scen = [s for s in scen if s["id"] in want]
    if args.validate:
        for s in scen:
            p = judge_prompt(s, "answer one", "answer two")
            assert "ANSWER A" in p and all(k in p for k in KEYS)
        assert len(load_scenarios(args.scenarios)) == 12
        print(f"ok: {len(scen)} scenarios, {len(RUBRIC)} rubric lines")
        return 0
    if args.judge in args.seats.split(","):
        sys.exit("the judge must run on a model that is not one of the seats")

    name = args.date + args.tag
    outdir = os.path.join(HERE, "results", name)
    args.baseline_rows = None
    if args.baseline:
        with open(os.path.join(HERE, "results", args.baseline, "summary.json")) as f:
            args.baseline_rows = json.load(f)
    os.makedirs(outdir, exist_ok=True)
    args.build = git_build(args.rt)
    if args.report_only:
        recs = {}
    else:
        with cf.ThreadPoolExecutor(args.jobs) as ex:
            futs = {ex.submit(run_scenario, args, s, outdir): s["id"] for s in scen}
            recs = {}
            for fu in cf.as_completed(futs):
                try:
                    recs[(futs[fu], args.mode)] = fu.result()
                except Exception as e:  # noqa: BLE001
                    log(f"{futs[fu]}: FAILED {e}")
    # The report covers every mode saved for this name: a Quick run and a Deep Think run
    # of the same day land in one file.
    for s in load_scenarios(args.scenarios):
        for p in (result_path(outdir, s["id"], "quick"), result_path(outdir, s["id"], "deep")):
            if not os.path.exists(p):
                continue
            with open(p) as f:
                rec = json.load(f)
            recs.setdefault((s["id"], rec.get("mode") or rec["table"].get("mode") or "none"), rec)
    by_id = {s["id"]: s for s in load_scenarios(args.scenarios)}
    rows = [summarize(by_id[sid], rec) for (sid, _), rec in sorted(recs.items(), key=lambda x: list(by_id).index(x[0][0]))]
    md = report(args, scen, rows, name)
    with open(os.path.join(HERE, "results", f"{name}.md"), "w") as f:
        f.write(md)
    with open(os.path.join(outdir, "summary.json"), "w") as f:
        json.dump(rows, f, indent=1)
    print(md)
    return 0


if __name__ == "__main__":
    sys.exit(main())
