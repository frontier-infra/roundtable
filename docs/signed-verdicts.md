# Signed council verdicts — design

Status: **draft, implemented behind opt-in flags.** Key management below is a set of
**options for the operator to choose from**, not a decision this document makes.

## Doctrine

The council advises and ratifies; it never verifies. Machine-internal, high-frequency
verification stays deterministic (verify_cmd exit codes, warden checks, local judges) and
never depends on external frontier models. The council is convened by the operator for
low-frequency, high-stakes judgment: architecture calls, migrations, adversarial plan
review, and ratification of major contracts when stakes warrant the spend. Its verdicts
become evidence only because the operator chose to sign them. Consensus is correlated
opinion, not a check against reality, which is why ground_truth stays unchecked. Nothing
may place a council call on an automated path. Ratified by Jason 2026-08-13.

## 1. The gap

The chair's verdict is prose. It reads well and nothing downstream can consume it:

```
VERDICT: CONSENSUS
**AGREED**
- ...
```

So the Decide layer emits an unsigned markdown opinion. Everything after it has to
either trust a human's summary or re-derive the judgment. Concretely, tonight's audit
found: `stack-demo`'s `goal.json` records *"council ratification not run"*, and The
Machine's Box-0 ratifier (`ratified_by != proposed_by`) is wired by hand — even though
Roundtable is the organ that should be producing that ratification.

Two artifacts close it:

| Artifact | What it is | When |
|---|---|---|
| `verdict.json` | Structured record of one council run — question, heads, hashes, chair ruling | `--verdict FILE` |
| `<verdict>.aar.json` | That record wrapped as an Ed25519-signed [AAR](https://agentscontrolplane.org) | `--sign` (needs a key) |

Both are **opt-in** and **fail-open**: a broken emitter or an unconfigured signer prints a
warning and never fails the council run. The transcript is unchanged.

```bash
roundtable ask -q "..." --rounds 3 --verdict ./verdict.json          # structured record
roundtable ask -q "..." --rounds 3 --verdict ./verdict.json --sign   # ...and sign it
roundtable sign ./verdict.json                                       # sign one later
```

The chair prompt contract is **unchanged**. `lib/verdict.py` parses the existing
`VERDICT: …` + `AGREED` / `REMAINING DISAGREEMENTS` output; `core.sh` additionally records
the decision it actually acted on (see §4).

## 2. `verdict.json`

`schema: https://roundtable.sh/schemas/council-verdict/v0.1` · `schema_version: "0.1"`

| Field | Type | Meaning |
|---|---|---|
| `schema`, `schema_version` | string | Schema id + version. |
| `hash_alg`, `hash_encoding` | string | `sha256` / `hex` — see §3. |
| `run.id` | string | `rt-<finished_at compact>-<question sha256[:10]>`. |
| `run.tool` | object | `{name, version}` of the engine that produced it. |
| `run.mode` | enum | `advisory` \| `deliberation`. |
| `run.rounds_max` / `rounds_run` | int | Cap requested vs rounds actually run. |
| `run.research` | bool | Whether web/tool use was enabled. |
| `run.started_at` / `finished_at` | RFC 3339 | UTC. |
| `run.transcript_path` | string\|null | Where the human transcript was retained (`--out`). |
| `question.text` | string | **Verbatim**, exactly as put to the heads. |
| `question.sha256` | hex | Over `question.text`, UTF-8, unmodified. |
| `question.context_file` / `context_sha256` | string\|null | Shared `-c` context, if any. |
| `heads[]` | array | Final-round per-head record (below). |
| `head_counts` | object | `{configured, answered, missing, timeout, empty, absent}`. |
| `chair` | object | `{model, status, round, output_sha256, warnings[]}`. |
| `verdict` | enum | `CONSENSUS` \| `CONTINUE` \| `NO_CHAIR`. |
| `agreed[]` / `disagreements[]` | string[] | Chair bullets, final round. |
| `rounds[]` | array | Per-round detail: heads, chair text hash, decision, timestamps. |

`heads[]` entry: `{name, label, model, status, answer_sha256, answer_chars, excerpt?}`.

- `status`: `answered` · `missing` (no key/CLI) · `timeout` · `empty` (provider returned
  nothing) · `absent` (no output file at all).
- `answer_sha256` is `null` for anything but `answered` — hashing the string
  `"__MISSING__ (no XAI_API_KEY)"` would be evidence of nothing.
- `model` is the id the council **asked for**. Codex runs the local CLI's own default and
  Grok can fall back to its CLI, so for those two the id served may differ.

`chair.status`: `ok` · `unreachable` (chair call failed — core.sh's own fallback path) ·
`unparsed` (output present, no `VERDICT:` token on line 1) · `absent` · `not_run`
(advisory mode). `NO_CHAIR` exists because an advisory run never convenes a chair; calling
that "CONTINUE" would imply a judgment nobody made.

## 3. Hashing

`sha256` over the answer text **stripped of leading/trailing whitespace**, UTF-8, hex in
`verdict.json`. Stripping is what makes a hash reproducible from the printed transcript:
`core.sh` renders each answer through `$(cat file)`, which drops trailing newlines.
Interior bytes are untouched.

The AAR re-encodes the *same digest* as **base64url**, because AAR §3.2 specifies
base64url for `checks[].response_sha256`. Same bytes, two encodings — do not compare the
strings, compare the digests.

**Bodies are never copied into the AAR.** `verdict.json` keeps a 160-char `excerpt` per
head for humans; the signed record carries hashes only.

## 4. Who decides the verdict

`core.sh` decides consensus by grepping line 1 of the chair output (`VERDICT: *CONSENSUS`)
and *acts* on it — it stops deliberating. If the emitter re-parsed that text independently,
the two could disagree and the artifact would claim something the engine never did.

So `core.sh` writes `chair.rN.decision` with the decision it acted on, and that value wins.
`verdict.py` still parses the text (it is more tolerant — it accepts `**VERDICT:** CONSENSUS`,
which the engine's grep does not) and any disagreement is recorded in `chair.warnings[]`
rather than silently overriding history. Parser drift becomes visible instead of becoming a
lie. Unreadable chair output records `CONTINUE` — the conservative direction.

## 5. The AAR wrapper

`--sign` builds an [Agent Attestation Record](https://agentscontrolplane.org) v0.02 and
shells out to the org's own reference signer (`node tools/aar.mjs sign`) — the same pattern
Proctor's `aar_emit.py` and machine-driver use, which makes Roundtable a third independent
producer of one record format rather than a fourth format.

```json
{
  "aar": "0.02",
  "subject":   "did:web:frontierinfra.org:roundtable",
  "principal": "did:web:frontierinfra.org",
  "task":   { "id": "rt-20260812T210031Z-7c78cb78fa", "claim": "<the question, verbatim>" },
  "verdict": "verified",
  "quality": "substantive",
  "ground_truth": "unchecked",
  "reason": "Roundtable deliberation run: 2/4 heads answered (grok,claude,minimax,gemini) over 2 of 3 round(s); the chair (claude-opus-4-8) judged the council substantively converged. This attests the deliberation and its record — not that the council's answer is true.",
  "checks": [ { "source": "…/verdict.json", "…": "…" } ],
  "verifier": { "id": "did:web:frontierinfra.org:roundtable:chair", "model": "claude-opus-4-8", "independence": "same_principal" },
  "issued": "2026-08-12T21:00:32Z",
  "sig": { "alg": "Ed25519", "by": "did:web:frontierinfra.org", "value": "…" }
}
```

### 5.1 Verdict mapping

| Council | AAR `verdict` | Why |
|---|---|---|
| `CONSENSUS` | `verified` | The chair judged the council substantively converged. |
| `CONTINUE` | `unclear` | No convergence. Nothing was *refuted*, so `not_verified` would overstate it — AAR reserves that for "checked and it does not hold". |
| chair unreachable / unparsed | `unclear` | No judgment was ever made. Recorded as `CONTINUE` upstream, so this row is the same row. |
| `NO_CHAIR` (advisory) | `unclear` | No chair was convened. |

Nothing maps to `not_verified` today. A council can decline to converge; it cannot
falsify. If a future chair contract adds an explicit "the claim is wrong" ruling, that is
the value it should map to.

`quality` (advisory per spec, never gating): `substantive` = CONSENSUS with ≥2 heads
answered · `none` = 0 heads answered · `shallow` = everything else.

### 5.2 `ground_truth` is `unchecked` — deliberately

A council reaching consensus is **not** ground truth. Seven models agreeing is seven
correlated opinions, not a check against real state. AAR §3.1 calls `unchecked` an honest
absence rather than a pass, and that is exactly the claim being made. Marking a council
verdict `confirmed` would be the fabrication this whole layer exists to prevent.

### 5.3 Identity scheme (proposal)

- `principal` — the org that answers for the council. Operator-configured; no default.
- `subject` — the council itself. Default `<principal>:roundtable`. Configurable, so an
  operator who wants per-machine or per-board councils can use
  `did:web:frontierinfra.org:roundtable:<machine>` without a code change.
- `verifier.id` — the chair. Default `<subject>:chair`, which satisfies AAR's L2 structural
  rule (`verifier.id != subject`). Read it for exactly what it is: the chair is a component
  of the council, so this is **structural** separation, not organizational independence.
  `independence` is therefore always `same_principal` — an organizational attestation.
- The council's composition is bound into the signature through `checks[]` (one entry per
  answering head) and the `reason` string, not through the DID.

### 5.4 `checks[]`

1. The `verdict.json` file itself — `response_sha256` over its bytes. This is what binds
   the whole structured record (every head hash, every bullet, all round detail) into one
   signature.
2. One entry per head that answered in the final round — `urn:roundtable:head:<name>`.
3. The chair's output — `urn:roundtable:chair:<model>`.

AAR §3.2's custody rule makes the issuer responsible for retaining the preimage. The
council's preimage is the transcript, and `$WORK` is deleted at exit — so `--sign` forces
the transcript to be retained (next to the verdict record when `--out` was not given) and
records its path in `run.transcript_path`.

### 5.5 What a signed council verdict is worth to Box-0

Box-0 wants `ratified_by != proposed_by`. A council AAR gives that a machine-checkable
form: the record names a subject that is not the proposing agent, commits to which models
answered, and is tamper-evident. What it does **not** give is organizational independence —
same principal, same operator, same keys. Treat it as *"a different party, under the same
roof, on the record"*. If a gate needs audit-grade independence it must require
`separate_principal`/`third_party`, and no configuration of this tool provides that today.

## 6. Key management — OPTIONS (operator decides)

Signing is off until an operator configures it, via `~/.config/roundtable/aar.config.json`
(or `ROUNDTABLE_AAR_*` env vars, or `roundtable sign` flags):

```json
{
  "aar_tool":  "~/code/agentcontrolplane/tools/aar.mjs",
  "aar_priv":  "~/.config/frontier-infra/keys/frontierinfra-org.jwk.json",
  "principal": "did:web:frontierinfra.org",
  "subject":   "did:web:frontierinfra.org:roundtable",
  "verifier":  "did:web:frontierinfra.org:roundtable:chair"
}
```

Precedence: CLI flag > env var > `aar.config.json` > `config.env`.

**Option A — reuse the existing `frontierinfra.org` did:web key.** The one Proctor's
`aar_emit.py` already signs with; `did.json` is already published, so any third party can
verify with no new infrastructure. *Cost:* the org signing key has to sit on every machine
that convenes a council, you cannot tell from the record which machine signed, and
revoking it invalidates Proctor's and machine-driver's attestations too.

**Option B — a per-machine council key.** `did:web:frontierinfra.org:councils:<machine>`
with its own keypair and its own published `did.json`. *Benefit:* compromise is scoped to
one machine, the signer says where the council ran, rotation is local. *Cost:* one more
`did.json` to publish and keep current per machine, and real key hygiene across the fleet.

**Option C — an operator key.** Only Jason's key signs, so a signed verdict means a human
promoted it — the Loom "operator promotes" rule expressed in cryptography, and the
strongest thing to hand Box-0. *Cost:* no unattended ratification, ever.

**Option D — stay unsigned by default (status quo, what ships).** Every run can emit
`verdict.json`; only machines explicitly configured sign. Structured and consumable
everywhere, cryptographic only where a key was deliberately placed.

These compose: A or B for automated runs, C layered on top for promotions that matter.

Storage is a separate axis. `aar.mjs` reads a **plaintext JWK file** (`chmod 600`) and takes
a path only — 1Password/Keychain would need a shim that materializes the key into a temp
file or a change to the signer. Worth knowing before picking a key that matters.

## 7. Failure behavior

| Situation | What happens |
|---|---|
| `verdict.py` missing / `python3` absent | Warning on stderr; transcript unaffected. |
| Emitter throws | Warning on stderr; transcript unaffected; no partial record. |
| Chair output unparseable | Record still lands: `chair.status=unparsed`, verdict `CONTINUE`, warnings recorded. |
| Signing not configured | `verdict.py sign` exits **3** with the reason; **no** `.aar.json` written; `verdict.json` untouched. |
| `aar.mjs` or `node` missing | Same as above — exit 3, nothing written. |
| Signer runs and fails | Exit 1; the **unsigned** AAR is left on disk (visible, verifiable-as-unsigned, signable later) — same precedent as `aar_emit.py`. |
| Any of the above during a run | `core.sh` warns and exits 0. The council result is never lost to attestation plumbing. |

## 8. Tests

```bash
bash lib/verdict.selftest.sh   # 41 assertions offline; +10 with a signer present
bash lib/core.selftest.sh      # engine, incl. --verdict / --sign wiring
```

No live model calls: every council run in the tests is a canned transcript written into a
temp dir. The signer round-trip (keygen → sign → verify → tamper-detect) runs offline when
`ROUNDTABLE_AAR_TOOL` points at a checkout of `aar.mjs`, and reports `SKIP` — never a
silent pass — when it does not.

## 9. Known limits / open questions

- **`task.claim` holds the question, not a claim.** AAR defines `task.claim` as "what the
  agent claimed it did". A council's unit of work *is* the question, so `verdict: verified`
  reads as "the council converged on this question", not "this answer is true" — which is
  why `reason` spells that out. If this proves confusing to consumers, the alternative is a
  claim like `"the council reached consensus on: <question>"` with the verbatim question
  living only in `verdict.json`.
- **Per-head observation times are round-grained**, not per-head. `observed_at` is when the
  round finished.
- **No `prior` chain / transparency log (L3).** Records are individually tamper-evident but
  not protected against retroactive rewrite after a key compromise.
- **The prompt each head saw is not hashed** — only its answer. Hashing the per-round
  prompt would make a round fully reproducible.
- **The MCP tool does not emit verdicts.** `roundtable(question, …)` still returns only a
  transcript. Wiring `--verdict` through it needs a decision this change did not make:
  where the file should land for a client that has no cwd of its own.
- **Nothing consumes this yet.** The Machine's Box-0 ratifier still needs wiring to read
  `verdict.json` / the AAR; that is a separate change in a separate repo.
