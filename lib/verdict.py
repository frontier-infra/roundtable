#!/usr/bin/env python3
"""verdict.py — structured, machine-consumable council verdicts for Roundtable.

The council's chair emits prose ("VERDICT: CONSENSUS" + AGREED / REMAINING
DISAGREEMENTS bullets). Prose is unusable downstream: a ratifier, a Box-0 gate or
an audit chain cannot consume markdown. This module turns one council run into

  1. ``verdict.json``  — a structured record of what the council was asked, who
     answered, what they said (hashes, not bodies), and what the chair ruled; and
  2. optionally an **Agent Attestation Record** (AAR, agentscontrolplane.org)
     wrapping that record as an Ed25519-signed statement, produced by shelling out
     to the org's own reference signer (``node tools/aar.mjs sign``) — the same
     pattern Proctor's ``aar_emit.py`` and machine-driver use, so Roundtable is a
     third independent producer of the same record format.

Design rules
------------
* **Zero dependencies.** python3 stdlib only; node is used only for signing and
  only if the operator configured a key.
* **The engine is the authority on the verdict.** ``core.sh`` decides CONSENSUS
  from line 1 of the chair output and acts on it (stops deliberating). When the
  engine records its decision (``chair.rN.decision``) that decision wins here, and
  a disagreeing local parse is reported as a warning rather than silently
  overriding what actually happened.
* **Fail-open at the call site.** Emission and signing never break a council run:
  ``core.sh`` calls this with ``|| warn``. Invoked directly, this CLI is honest
  about failures (non-zero exit) so a human running ``roundtable sign`` sees them.
* **Attest the deliberation, not the truth.** A council reaching consensus is not
  ground truth, so the AAR is emitted with ``ground_truth: "unchecked"``. See
  docs/signed-verdicts.md.

Subcommands
-----------
  verdict.py emit --run-dir DIR --out verdict.json [...]   build the record
  verdict.py sign verdict.json [--out record.aar.json]     wrap + Ed25519-sign it

Run-dir layout (written by core.sh; a test can fabricate it offline):
  <head>.out            advisory-mode head output (raw)
  <head>.r<N>.out       deliberation-mode head output for round N (raw)
  chair.r<N>.out        chair output for round N (raw prose)
  chair.r<N>.status     "ok" | "unreachable"                       (optional)
  chair.r<N>.decision   "CONSENSUS" | "CONTINUE" — engine's reading (optional)
  r<N>.finished         RFC 3339 stamp, heads of round N done      (optional)
  chair.r<N>.finished   RFC 3339 stamp, chair of round N done      (optional)

Spec: docs/signed-verdicts.md · AAR: specs/aar-agent-attestation-record.md (v0.02)
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone

SCHEMA_ID = "https://roundtable.sh/schemas/council-verdict/v0.1"
SCHEMA_VERSION = "0.1"
AAR_SPEC_VERSION = "0.02"
DEFAULT_TOOL_VERSION = "0.1.0"

# Exit codes: 0 ok · 1 failure · 2 usage · 3 not configured (nothing to sign with).
EX_OK, EX_FAIL, EX_USAGE, EX_UNCONFIGURED = 0, 1, 2, 3

# Sentinels core.sh writes into a head's output file when it did not answer.
HEAD_SENTINELS = (("__MISSING__", "missing"), ("__TIMEOUT__", "timeout"), ("__EMPTY__", "empty"))

MAX_ITEMS = 100          # bullets kept per chair section
MAX_ITEM_CHARS = 1000    # per bullet
EXCERPT_CHARS = 160      # per head, verdict.json only (never copied into the AAR)


# ── helpers ─────────────────────────────────────────────────────────────────

def now_utc() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def digest(text: str) -> bytes:
    """SHA-256 over the answer text with leading/trailing whitespace stripped, UTF-8.

    Stripping is what makes the hash reproducible from the printed transcript:
    core.sh renders each answer through ``$(cat file)``, which drops trailing
    newlines. Interior bytes are untouched.
    """
    return hashlib.sha256(text.strip().encode("utf-8")).digest()


def hex_of(text: str) -> str:
    return digest(text).hex()


def b64u(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).decode("ascii").rstrip("=")


def read_text(path: str):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return None


def one_line(text: str, limit: int) -> str:
    flat = " ".join(text.split())
    return flat if len(flat) <= limit else flat[: limit - 1] + "…"


def as_int(value, fallback: int) -> int:
    """Tolerant int() — a malformed flag must not cost the run its verdict record."""
    try:
        return int(str(value).strip())
    except (TypeError, ValueError):
        return fallback


# ── chair parsing ───────────────────────────────────────────────────────────

# core.sh's own rule is `head -1 | grep -qi "VERDICT: *CONSENSUS"` — a substring
# match on line 1. We match the same shape but also tolerate markdown decoration
# ("**VERDICT:** CONSENSUS"), and flag any disagreement with the engine.
VERDICT_RE = re.compile(r"VERDICT\s*:?\s*\**\s*:?\s*(CONSENSUS|CONTINUE)\b", re.IGNORECASE)
UNREACHABLE_RE = re.compile(r"chair unavailable", re.IGNORECASE)
BULLET_RE = re.compile(r"^\s*(?:[-*+•‣●]|\d+[.)])\s+(.*\S)\s*$")
RULE_RE = re.compile(r"^\s*(?:-{3,}|_{3,}|\*{3,})\s*$")


def _strip_decor(line: str) -> str:
    return line.strip().strip("#*_`>~ \t").strip()


AGREED_HEAD_RE = re.compile(r"^(AGREED|AGREEMENTS?|POINTS? OF AGREEMENT)\b")
DISAGREE_HEAD_RE = re.compile(r"^(REMAINING|DISAGREEMENTS?|OPEN DISAGREEMENTS?)\b")


def _section_of(line: str):
    """Return 'agreed' | 'disagreements' | None plus any inline remainder.

    A heading must *start* with the section word (and be short) — otherwise a
    prose line like "there was some disagreement about naming" would silently
    re-section everything that follows it.
    """
    bare = _strip_decor(line)
    if not bare or len(bare) > 200:
        return None, ""
    head, _, rest = bare.partition(":")
    head_norm = _strip_decor(head).upper()
    if not head_norm or len(head_norm) > 48:
        return None, ""
    if AGREED_HEAD_RE.match(head_norm):
        return "agreed", rest.strip()
    if DISAGREE_HEAD_RE.match(head_norm):
        return "disagreements", rest.strip()
    return None, ""


def parse_chair(text: str) -> dict:
    """Parse one chair transcript. Never raises; reports what it could not read.

    Returns {verdict, agreed[], disagreements[], warnings[], unreachable}.
    ``verdict`` is None when line 1 carried no VERDICT token.
    """
    out = {"verdict": None, "agreed": [], "disagreements": [], "warnings": [], "unreachable": False}
    if text is None:
        out["warnings"].append("chair output file missing")
        return out
    lines = text.splitlines()
    nonempty = [ln for ln in lines if ln.strip()]
    if not nonempty:
        out["warnings"].append("chair output was empty")
        return out

    match = VERDICT_RE.search(nonempty[0])
    if match:
        out["verdict"] = match.group(1).upper()
    else:
        # Not on line 1 — look further down so the record can still say what the
        # chair meant, but keep the warning: the engine only reads line 1.
        later = None
        for ln in nonempty[1:6]:
            later = VERDICT_RE.search(ln)
            if later:
                break
        if later:
            out["verdict"] = later.group(1).upper()
            out["warnings"].append(
                "VERDICT token not on line 1 (the engine only reads line 1 — its "
                "control flow may differ from this parse)")
        else:
            out["warnings"].append("no 'VERDICT: CONSENSUS|CONTINUE' token found in the chair output")

    if UNREACHABLE_RE.search(text):
        out["unreachable"] = True

    sections = {"agreed": {"bullets": [], "prose": []}, "disagreements": {"bullets": [], "prose": []}}
    current = None
    for ln in lines[1:] if lines else []:
        if RULE_RE.match(ln):
            current = None
            continue
        name, inline = _section_of(ln)
        if name and not BULLET_RE.match(ln):
            current = name
            if inline:
                sections[current]["bullets"].append(inline)
            continue
        if current is None:
            continue
        bullet = BULLET_RE.match(ln)
        if bullet:
            sections[current]["bullets"].append(bullet.group(1))
        elif ln.strip():
            sections[current]["prose"].append(ln.strip())

    for name in ("agreed", "disagreements"):
        got = sections[name]["bullets"]
        if not got and sections[name]["prose"]:
            got = [" ".join(sections[name]["prose"])]
            out["warnings"].append("%s section had no bullets — captured its prose as one item" % name)
        elif not got:
            out["warnings"].append("%s section not found or empty" % name)
        if len(got) > MAX_ITEMS:
            out["warnings"].append("%s section truncated to %d items" % (name, MAX_ITEMS))
            got = got[:MAX_ITEMS]
        out[name] = [one_line(item, MAX_ITEM_CHARS) for item in got]
    return out


# ── head parsing ────────────────────────────────────────────────────────────

def classify_head(raw):
    """(status, text) for one head's raw output file."""
    if raw is None:
        return "absent", ""
    body = raw.strip()
    if not body:
        return "empty", ""
    for sentinel, status in HEAD_SENTINELS:
        if body.startswith(sentinel):
            return status, ""
    return "answered", body


def load_heads_file(path: str):
    """name<TAB>label<TAB>model per line. Missing file → []."""
    rows = []
    text = read_text(path) if path else None
    if text is None:
        return rows
    for line in text.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        rows.append({
            "name": parts[0].strip(),
            "label": parts[1].strip() if len(parts) > 1 else parts[0].strip(),
            "model": parts[2].strip() if len(parts) > 2 else "",
        })
    return rows


def head_file(run_dir: str, name: str, rnd: int, mode: str) -> str:
    if mode == "advisory":
        return os.path.join(run_dir, "%s.out" % name)
    per_round = os.path.join(run_dir, "%s.r%d.out" % (name, rnd))
    if os.path.exists(per_round):
        return per_round
    return os.path.join(run_dir, "%s.out" % name)


def round_record(run_dir: str, heads, rnd: int, mode: str, fallback_ts: str) -> dict:
    entries = []
    for head in heads:
        raw = read_text(head_file(run_dir, head["name"], rnd, mode))
        status, body = classify_head(raw)
        entry = {
            "name": head["name"],
            "label": head["label"],
            "model": head["model"],
            "status": status,
            "answer_sha256": hex_of(body) if status == "answered" else None,
            "answer_chars": len(body) if status == "answered" else 0,
        }
        if status == "answered":
            entry["excerpt"] = one_line(body, EXCERPT_CHARS)
        entries.append(entry)

    chair = {"status": "not_run", "decision": None, "output_sha256": None,
             "agreed": [], "disagreements": [], "warnings": [], "observed_at": None}
    if mode != "advisory":
        chair_text = read_text(os.path.join(run_dir, "chair.r%d.out" % rnd))
        parsed = parse_chair(chair_text)
        engine = (read_text(os.path.join(run_dir, "chair.r%d.decision" % rnd)) or "").strip().upper() or None
        status_file = (read_text(os.path.join(run_dir, "chair.r%d.status" % rnd)) or "").strip().lower() or None

        warnings = list(parsed["warnings"])
        if engine and parsed["verdict"] and engine != parsed["verdict"]:
            warnings.append("engine recorded %s but this parse read %s — the engine's decision is authoritative"
                            % (engine, parsed["verdict"]))
        decision = engine or parsed["verdict"] or "CONTINUE"
        if not engine and not parsed["verdict"]:
            warnings.append("no verdict could be read — recorded conservatively as CONTINUE")

        if status_file in ("ok", "unreachable"):
            state = status_file
        elif chair_text is None:
            state = "absent"
        elif parsed["unreachable"]:
            state = "unreachable"
        elif parsed["verdict"] is None:
            state = "unparsed"
        else:
            state = "ok"

        chair = {
            "status": state,
            "decision": decision,
            "output_sha256": hex_of(chair_text) if chair_text else None,
            "agreed": parsed["agreed"],
            "disagreements": parsed["disagreements"],
            "warnings": warnings,
            "observed_at": (read_text(os.path.join(run_dir, "chair.r%d.finished" % rnd)) or "").strip() or fallback_ts,
        }

    return {
        "round": rnd,
        "observed_at": (read_text(os.path.join(run_dir, "r%d.finished" % rnd)) or "").strip() or fallback_ts,
        "heads": entries,
        "chair": chair,
    }


# ── emit ────────────────────────────────────────────────────────────────────

def build_record(args) -> dict:
    question = read_text(args.question_file)
    if question is None:
        raise SystemExit("verdict: cannot read --question-file %s" % args.question_file)
    heads = load_heads_file(args.heads_file)
    mode = "advisory" if args.mode == "advisory" else "deliberation"
    rounds_run = max(1, as_int(args.rounds_run, 1))
    finished = args.finished_at or now_utc()
    started = args.started_at or finished

    rounds = [round_record(args.run_dir, heads, r, mode, finished) for r in range(1, rounds_run + 1)]
    final = rounds[-1]

    if mode == "advisory":
        # No chair is convened in advisory mode, so the record must not name one —
        # a signed record naming a verifier model that never ran would be a lie.
        verdict, chair_block = "NO_CHAIR", {
            "model": None, "status": "not_run", "round": None,
            "output_sha256": None, "warnings": [],
        }
        agreed, disagreements = [], []
    else:
        chair = final["chair"]
        verdict = chair["decision"] or "CONTINUE"
        if chair["status"] in ("unreachable", "absent"):
            verdict = "CONTINUE"
        agreed, disagreements = chair["agreed"], chair["disagreements"]
        chair_block = {
            "model": args.chair_model or None,
            "status": chair["status"],
            "round": final["round"],
            "output_sha256": chair["output_sha256"],
            "warnings": chair["warnings"],
        }

    counts = {"configured": len(heads)}
    for status in ("answered", "missing", "timeout", "empty", "absent"):
        counts[status] = sum(1 for h in final["heads"] if h["status"] == status)

    qsha = hashlib.sha256(question.encode("utf-8")).hexdigest()
    run_id = args.run_id or "rt-%s-%s" % (re.sub(r"[^0-9A-Za-z]", "", finished), qsha[:10])

    record = {
        "schema": SCHEMA_ID,
        "schema_version": SCHEMA_VERSION,
        "hash_alg": "sha256",
        "hash_encoding": "hex",
        "run": {
            "id": run_id,
            "tool": {"name": "roundtable", "version": args.tool_version or DEFAULT_TOOL_VERSION},
            "mode": mode,
            "rounds_max": max(rounds_run, as_int(args.rounds_max, rounds_run)),
            "rounds_run": rounds_run,
            "research": as_int(args.research, 0) == 1,
            "started_at": started,
            "finished_at": finished,
            "transcript_path": os.path.abspath(args.transcript) if args.transcript else None,
        },
        "question": {
            "text": question,
            "sha256": qsha,
            "context_file": os.path.abspath(args.context_file) if args.context_file else None,
            "context_sha256": (hashlib.sha256((read_text(args.context_file) or "").encode("utf-8")).hexdigest()
                               if args.context_file and os.path.exists(args.context_file) else None),
        },
        "heads": final["heads"],
        "head_counts": counts,
        "chair": chair_block,
        "verdict": verdict,
        "agreed": agreed,
        "disagreements": disagreements,
        "rounds": rounds,
    }
    return record


def cmd_emit(args) -> int:
    record = build_record(args)
    out = args.out
    parent = os.path.dirname(os.path.abspath(out))
    if parent:
        os.makedirs(parent, exist_ok=True)
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(json.dumps(record, indent=2, ensure_ascii=False) + "\n")
    warnings = list(record["chair"].get("warnings") or [])
    for w in warnings:
        sys.stderr.write("roundtable verdict: %s\n" % w)
    if not args.quiet:
        print(out)
    return EX_OK


# ── sign ────────────────────────────────────────────────────────────────────

# CONSENSUS  → verified : the chair judged the council substantively converged.
# CONTINUE   → unclear  : no convergence — nothing was refuted, so not "not_verified".
# NO_CHAIR   → unclear  : advisory run; no consensus judgment was ever made.
# chair unreachable/unparsed → unclear (handled by the CONTINUE row).
VERDICT_MAP = {"CONSENSUS": "verified", "CONTINUE": "unclear", "NO_CHAIR": "unclear"}

AAR_CONFIG_KEYS = ("aar_tool", "aar_priv", "subject", "principal", "verifier")
ENV_KEYS = {
    "aar_tool": "ROUNDTABLE_AAR_TOOL",
    "aar_priv": "ROUNDTABLE_AAR_PRIV",
    "subject": "ROUNDTABLE_AAR_SUBJECT",
    "principal": "ROUNDTABLE_AAR_PRINCIPAL",
    "verifier": "ROUNDTABLE_AAR_VERIFIER",
}


def config_home() -> str:
    return os.path.join(os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"), "roundtable")


def read_config_env(keys) -> dict:
    """Pick ROUNDTABLE_AAR_* out of config.env without a dotenv parser (same
    grep-a-line approach bin/roundtable uses). Last assignment wins."""
    found = {}
    text = read_text(os.path.join(config_home(), "config.env"))
    if text is None:
        return found
    wanted = {ENV_KEYS[k]: k for k in keys}
    for line in text.splitlines():
        m = re.match(r"^\s*(?:export\s+)?([A-Z0-9_]+)\s*=\s*(.*?)\s*$", line)
        if not m or m.group(1) not in wanted:
            continue
        val = m.group(2)
        if len(val) >= 2 and val[0] == val[-1] and val[0] in "\"'":
            val = val[1:-1]
        if val:
            found[wanted[m.group(1)]] = val
    return found


def resolve_signing_config(args) -> dict:
    """CLI flag > process env > aar.config.json > config.env."""
    cfg = {}
    cfg.update(read_config_env(AAR_CONFIG_KEYS))
    path = args.config or os.path.join(config_home(), "aar.config.json")
    raw = read_text(path)
    if raw:
        try:
            loaded = json.loads(raw)
            cfg.update({k: v for k, v in loaded.items() if k in AAR_CONFIG_KEYS and v})
        except ValueError as exc:
            sys.stderr.write("roundtable sign: ignoring unreadable %s (%s)\n" % (path, exc))
    for key, env in ENV_KEYS.items():
        if os.environ.get(env):
            cfg[key] = os.environ[env]
    for key in AAR_CONFIG_KEYS:
        flag = getattr(args, key, None)
        if flag:
            cfg[key] = flag
    for key in ("aar_tool", "aar_priv"):
        if cfg.get(key):
            cfg[key] = os.path.expanduser(cfg[key])
    return cfg


def quality_of(record: dict) -> str:
    answered = record.get("head_counts", {}).get("answered", 0)
    if answered == 0:
        return "none"
    if record.get("verdict") == "CONSENSUS" and answered >= 2:
        return "substantive"
    return "shallow"


def build_aar(record: dict, verdict_path: str, cfg: dict) -> dict:
    subject = cfg.get("subject") or "%s:roundtable" % cfg["principal"]
    verifier_id = cfg.get("verifier") or "%s:chair" % subject
    run, question = record["run"], record["question"]
    chair = record.get("chair") or {}
    final_round = record["rounds"][-1] if record.get("rounds") else {"observed_at": run["finished_at"], "heads": []}
    names = ",".join(h["name"] for h in record.get("heads", [])) or "none"
    answered = record.get("head_counts", {}).get("answered", 0)

    with open(verdict_path, "rb") as fh:
        verdict_digest = hashlib.sha256(fh.read()).digest()
    checks = [{
        "source": os.path.abspath(verdict_path),
        "query": "sha256 of the Roundtable council verdict record (%s)" % record.get("schema", SCHEMA_ID),
        "observed_at": run["finished_at"],
        "response_sha256": b64u(verdict_digest),
    }]
    for head in final_round.get("heads", []):
        if head.get("status") != "answered" or not head.get("answer_sha256"):
            continue
        checks.append({
            "source": "urn:roundtable:head:%s" % head["name"],
            "query": "council round %s answer from %s" % (final_round.get("round", 1), head.get("model") or head["name"]),
            "observed_at": final_round.get("observed_at") or run["finished_at"],
            "response_sha256": b64u(bytes.fromhex(head["answer_sha256"])),
        })
    if chair.get("output_sha256"):
        checks.append({
            "source": "urn:roundtable:chair:%s" % (chair.get("model") or "unknown"),
            "query": "chair adjudication of round %s" % chair.get("round"),
            "observed_at": (final_round.get("chair") or {}).get("observed_at") or run["finished_at"],
            "response_sha256": b64u(bytes.fromhex(chair["output_sha256"])),
        })

    if record["verdict"] == "CONSENSUS":
        ruling = "the chair (%s) judged the council substantively converged" % (chair.get("model") or "unknown")
    elif record["verdict"] == "NO_CHAIR":
        ruling = "advisory run — no chair was convened, so no consensus judgment was made"
    elif chair.get("status") in ("unreachable", "absent"):
        ruling = "the chair was unreachable, so no consensus judgment was made"
    else:
        ruling = "the chair (%s) found material disagreement remaining" % (chair.get("model") or "unknown")

    return {
        "aar": AAR_SPEC_VERSION,
        "subject": subject,
        "principal": cfg["principal"],
        "task": {"id": run["id"], "claim": question["text"]},
        "verdict": VERDICT_MAP.get(record["verdict"], "unclear"),
        "quality": quality_of(record),
        "ground_truth": "unchecked",
        "reason": ("Roundtable %s run: %d/%d heads answered (%s) over %s of %s round(s); %s. "
                   "This attests the deliberation and its record — not that the council's answer is true."
                   % (run["mode"], answered, record["head_counts"].get("configured", 0), names,
                      run["rounds_run"], run["rounds_max"], ruling)),
        "checks": checks,
        "verifier": {
            "id": verifier_id,
            "model": chair.get("model") or "none",
            "independence": "same_principal",
        },
        "issued": now_utc(),
    }


def cmd_sign(args) -> int:
    raw = read_text(args.verdict)
    if raw is None:
        sys.stderr.write("roundtable sign: cannot read %s\n" % args.verdict)
        return EX_FAIL
    try:
        record = json.loads(raw)
    except ValueError as exc:
        sys.stderr.write("roundtable sign: %s is not valid JSON (%s)\n" % (args.verdict, exc))
        return EX_FAIL
    if not record.get("verdict") or not record.get("run"):
        sys.stderr.write("roundtable sign: %s is not a Roundtable verdict record\n" % args.verdict)
        return EX_FAIL

    cfg = resolve_signing_config(args)
    missing = [k for k in ("aar_tool", "aar_priv", "principal") if not cfg.get(k)]
    if missing:
        sys.stderr.write(
            "roundtable sign: signing not configured (missing %s). Set them in %s/aar.config.json "
            "or pass --aar-tool/--aar-priv/--principal. The unsigned verdict record is unaffected.\n"
            % (", ".join(missing), config_home()))
        return EX_UNCONFIGURED
    absent = [cfg[k] for k in ("aar_tool", "aar_priv") if not os.path.exists(cfg[k])]
    if absent:
        sys.stderr.write("roundtable sign: cannot sign — not found: %s\n" % ", ".join(absent))
        return EX_UNCONFIGURED
    node = shutil.which("node")
    if not node:
        sys.stderr.write("roundtable sign: cannot sign — 'node' is not on PATH (the AAR signer needs Node >= 20)\n")
        return EX_UNCONFIGURED

    out = args.out or re.sub(r"(\.json)?$", ".aar.json", args.verdict, count=1)
    try:
        aar = build_aar(record, args.verdict, cfg)
    except (KeyError, TypeError, ValueError, OSError) as exc:
        sys.stderr.write("roundtable sign: could not build the AAR (%s)\n" % exc)
        return EX_FAIL
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(json.dumps(aar, indent=2, ensure_ascii=False) + "\n")

    try:
        proc = subprocess.run([node, cfg["aar_tool"], "sign", out, "--priv", cfg["aar_priv"]],
                              capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError) as exc:
        sys.stderr.write("roundtable sign: signer failed to run (%s) — unsigned record left at %s\n" % (exc, out))
        return EX_FAIL
    if proc.returncode != 0:
        tail = (proc.stdout + proc.stderr).strip().splitlines()
        sys.stderr.write("roundtable sign: signer exited %d (%s) — unsigned record left at %s\n"
                         % (proc.returncode, tail[-1] if tail else "no output", out))
        return EX_FAIL
    if not args.quiet:
        print(out)
    return EX_OK


# ── CLI ─────────────────────────────────────────────────────────────────────

def main(argv) -> int:
    parser = argparse.ArgumentParser(prog="verdict.py", description="Roundtable structured council verdicts")
    sub = parser.add_subparsers(dest="cmd")

    emit = sub.add_parser("emit", help="build verdict.json from a council run directory")
    emit.add_argument("--run-dir", required=True)
    emit.add_argument("--out", required=True)
    emit.add_argument("--question-file", required=True)
    emit.add_argument("--heads-file", default="")
    emit.add_argument("--mode", default="deliberation", choices=("advisory", "deliberation"))
    emit.add_argument("--rounds-max", default=1)
    emit.add_argument("--rounds-run", default=1)
    emit.add_argument("--chair-model", default="")
    emit.add_argument("--research", default="0")
    emit.add_argument("--started-at", default="")
    emit.add_argument("--finished-at", default="")
    emit.add_argument("--context-file", default="")
    emit.add_argument("--transcript", default="")
    emit.add_argument("--tool-version", default=DEFAULT_TOOL_VERSION)
    emit.add_argument("--run-id", default="")
    emit.add_argument("--quiet", action="store_true")

    sign = sub.add_parser("sign", help="wrap a verdict record as an Ed25519-signed AAR")
    sign.add_argument("verdict")
    sign.add_argument("--out", default="")
    sign.add_argument("--config", default="")
    sign.add_argument("--aar-tool", dest="aar_tool", default="")
    sign.add_argument("--aar-priv", dest="aar_priv", default="")
    sign.add_argument("--subject", default="")
    sign.add_argument("--principal", default="")
    sign.add_argument("--verifier", default="")
    sign.add_argument("--quiet", action="store_true")

    args = parser.parse_args(argv[1:])
    if args.cmd == "emit":
        return cmd_emit(args)
    if args.cmd == "sign":
        return cmd_sign(args)
    parser.print_usage(sys.stderr)
    return EX_USAGE


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except BrokenPipeError:
        sys.exit(EX_OK)
