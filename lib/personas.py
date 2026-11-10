#!/usr/bin/env python3
"""personas.py: seat roles for the bash engine. Same packs and rules as the Rust engine
(desktop/engine/src/personas.rs). Standard library only.

A persona sits on an AI seat: its brief goes in front of that seat's prompt and its name
in front of the seat's label. No persona is a Generic seat, as before.

  personas.py seats SPEC HEADS_CSV OUTDIR   write OUTDIR/seats.tsv and OUTDIR/brief.N
  personas.py list [--json]                 show packs, presets and personas

SPEC is a comma list. Each item is a preset id, a pack id, a persona id or name, or
`generic`, optionally ending in @<head>. The Frontier labs pack is hidden by
ROUNDTABLE_HIDE_LAB_PACK=1 or by show_lab_pack=false in the app's settings.json.
"""
import json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
BUILTIN_DIR = os.path.join(HERE, "..", "personas")
BUILTIN_ORDER = ["frontier-labs", "business-roles", "expert-panels"]
HEADS = ["claude", "openai", "grok", "glm", "minimax", "gemini", "codex", "qwen", "kimi", "opencode", "cursor"]
MAX_SEATS = 16
GENERIC = "generic"


def disclaimer(lab):
    return "Simulated perspective drawn from public positions. Not affiliated with or endorsed by %s." % lab


def slug(s):
    out = ""
    for c in s.strip():
        if c.isascii() and c.isalnum():
            out += c.lower()
        elif out and not out.endswith("-"):
            out += "-"
    return out.rstrip("-")


def truthy(v):
    return (v or "").strip().lower() in ("1", "true", "yes", "on")


def data_dir():
    d = os.environ.get("ROUNDTABLE_DATA_DIR")
    if d:
        return d
    home = os.path.expanduser("~")
    if sys.platform == "darwin":
        return os.path.join(home, "Library", "Application Support", "sh.roundtable.app")
    if os.name == "nt":
        return os.path.join(os.environ.get("APPDATA", home), "sh.roundtable.app")
    return os.path.join(os.environ.get("XDG_DATA_HOME") or os.path.join(home, ".local", "share"), "sh.roundtable.app")


def labs_hidden():
    if truthy(os.environ.get("ROUNDTABLE_HIDE_LAB_PACK")):
        return True
    try:
        with open(os.path.join(data_dir(), "settings.json")) as f:
            return json.load(f).get("show_lab_pack", True) is False
    except Exception:
        return False


def user_dirs():
    cfg = os.environ.get("XDG_CONFIG_HOME") or os.path.join(os.path.expanduser("~"), ".config")
    v = [os.path.join(data_dir(), "personas"), os.path.join(cfg, "roundtable", "personas")]
    if os.environ.get("ROUNDTABLE_PERSONAS_DIR"):
        v.append(os.environ["ROUNDTABLE_PERSONAS_DIR"])
    return v


def validate(p):
    """The pack cleaned up, or None when it is not valid (same rules as the Rust engine)."""
    if not isinstance(p, dict) or not str(p.get("id", "")).strip():
        return None
    p["id"] = p["id"].strip()
    if not str(p.get("name", "")).strip():
        p["name"] = p["id"]
    seen = set()
    personas = p.get("personas") or []
    for x in personas:
        x["name"] = str(x.get("name", "")).strip()
        x["id"] = str(x.get("id", "")).strip() or slug(x["name"])
        if not x["id"] or not x["name"] or x["id"] in seen:
            return None
        seen.add(x["id"])
        if not str(x.get("ai") or "").strip():
            x.pop("ai", None)
        elif x["ai"] not in HEADS:
            return None
    p["personas"] = personas
    p["presets"] = p.get("presets") or []
    return p


def load_library():
    hide = labs_hidden()
    packs = []
    for name in BUILTIN_ORDER:
        path = os.path.join(BUILTIN_DIR, name + ".json")
        if not os.path.exists(path):
            continue
        with open(path) as f:
            packs.append(json.load(f))
    for d in user_dirs():
        try:
            files = sorted(f for f in os.listdir(d) if f.endswith(".json"))
        except OSError:
            continue
        for fn in files:
            try:
                with open(os.path.join(d, fn)) as f:
                    p = validate(json.load(f))
            except Exception:
                p = None
            if p and not any(q["id"] == p["id"] for q in packs):
                packs.append(p)
    if hide:
        packs = [p for p in packs if not p.get("labs")]
        for p in packs:
            p["personas"] = [x for x in p.get("personas", []) if not x.get("lab")]
    return packs


def all_personas(packs):
    return [x for p in packs for x in p.get("personas", [])]


def find_persona(packs, key):
    k = key.strip()
    ks = slug(k)
    xs = all_personas(packs)
    for test in (lambda x: x["id"] == k, lambda x: x["name"].lower() == k.lower(), lambda x: x["id"] == ks or slug(x["name"]) == ks):
        for x in xs:
            if test(x):
                return x
    return None


def find_preset(packs, key):
    for p in packs:
        for pr in p.get("presets", []):
            if pr["id"] == key.strip():
                return pr
    return None


def find_pack(packs, key):
    for p in packs:
        if p["id"] == key.strip():
            return p
    return None


def resolve(spec, heads, packs):
    """[(head, persona or None)], or ValueError with the reason."""
    if not spec.strip() or spec.strip().lower() == GENERIC:
        return [(h, None) for h in heads]
    if not heads:
        raise ValueError("No AI is at the table to seat the personas.")
    want = []
    for item in [s.strip() for s in spec.split(",") if s.strip()]:
        key, forced = item, None
        if "@" in item:
            key, at = item.rsplit("@", 1)
            key, at = key.strip(), at.strip()
            if at not in HEADS:
                raise ValueError("Unknown AI '%s' in '%s'." % (at, item))
            forced = at
        if key.lower() == GENERIC:
            want.append((None, forced))
            continue
        pr, pk, pe = find_preset(packs, key), find_pack(packs, key), find_persona(packs, key)
        if pr:
            for s in pr["seats"]:
                x = find_persona(packs, s)
                if not x:
                    raise ValueError("The preset '%s' names an unknown persona '%s'." % (pr["id"], s))
                want.append((x, forced))
        elif pk:
            want.extend((x, forced) for x in pk.get("personas", []))
        elif pe:
            want.append((pe, forced))
        else:
            raise ValueError("Unknown persona '%s'. Run `roundtable personas` to see them." % key)
    if len(want) > MAX_SEATS:
        raise ValueError("That is %d seats. The most is %d." % (len(want), MAX_SEATS))
    seats = [f or (p.get("ai") if p and p.get("ai") in heads else None) for p, f in want]
    for i in range(len(seats)):
        if seats[i] is None:
            seats[i] = min(heads, key=lambda h: (sum(1 for s in seats if s == h), heads.index(h)))
    return [(h, p) for (p, _), h in zip(want, seats)]


def seat_brief(p):
    s = "## YOUR SEAT\nYou sit at this roundtable as %s" % p["name"]
    if p.get("expertise", "").strip():
        s += " (%s)" % p["expertise"].strip()
    s += ".\n"
    if p.get("brief", "").strip():
        s += p["brief"].strip() + "\n"
    s += "Answer from this seat. Stay in role, and say so plainly when a point is outside this seat's expertise.\n"
    if p.get("lab"):
        s += ("This is a simulated perspective drawn from public positions. Do not claim to speak for %s "
              "or for any person, and do not invent quotes.\n") % p["lab"]
    return s + "\n"


def cmd_seats(spec, heads_csv, outdir):
    heads = [h for h in heads_csv.split(",") if h]
    try:
        seats = resolve(spec, heads, load_library())
    except ValueError as e:
        sys.stderr.write("ERROR: %s\n" % e)
        return 2
    with open(os.path.join(outdir, "seats.tsv"), "w") as tsv:
        for i, (h, p) in enumerate(seats):
            name = p["name"].replace("\t", " ") if p else "-"
            disc = disclaimer(p["lab"]) if p and p.get("lab") else "-"
            tsv.write("%s\t%s\t%s\n" % (h, name, disc))
            with open(os.path.join(outdir, "brief.%d" % i), "w") as f:
                f.write(seat_brief(p) if p else "")
    return 0


def cmd_list(as_json):
    packs = load_library()
    if as_json:
        print(json.dumps({"packs": packs, "labs_hidden": labs_hidden()}))
        return 0
    print("Generic: plain seats, one per AI (--personas generic).")
    for p in packs:
        print()
        print("%s (%s)" % (p["name"], p["id"]))
        for x in p.get("personas", []):
            ai = ", prefers %s" % x["ai"] if x.get("ai") else ""
            print("  %-22s %s%s" % (x["id"], x["name"], ai))
        for pr in p.get("presets", []):
            print("  preset %-15s %s" % (pr["id"], ", ".join(pr["seats"])))
    print()
    if labs_hidden():
        print("The Frontier labs pack is hidden.")
    else:
        print("Lab seats are simulated perspectives drawn from public positions, not affiliated with or endorsed by any lab.")
    return 0


def main(argv):
    if len(argv) >= 4 and argv[0] == "seats":
        return cmd_seats(argv[1], argv[2], argv[3])
    if not argv or argv[0] == "list":
        return cmd_list("--json" in argv)
    if argv[0] == "--json":
        return cmd_list(True)
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
