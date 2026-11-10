# Usefulness bench

Is Roundtable more useful than asking one good AI? This bench checks that on
twelve real decisions a small-business owner or technical lead faces.

## What it does

1. Each scenario in `scenarios.json` is a rough question, the way a person types
   it, plus hidden facts a good answer should surface. Two scenarios
   (`database-pick`, `funding-choice`) are "it depends" questions: the right
   answer names what to find out first.
2. The question goes to the council (`roundtable ask --heads <seats> --rounds 2`)
   and to one single AI (`--heads <single>`), with nothing else added.
3. A judge on a model that is not a seat scores both answers 1 to 5 on seven
   lines and picks the one it would rather act on. It runs twice per scenario,
   with the order swapped, to cancel position bias.
4. `results/<date>.md` gets per-scenario scores and the totals, and
   `results/<date>/summary.json` the same rows as data. Both are committed.
   `results/<date>/<id>.json` keeps every answer, transcript and judge reply.
   Those stay local (gitignored): a seat that runs through a signed-in app can
   echo the runner's own agent setup into its answer.

The Roundtable answer the judge reads is what a person reads at the end of a
run. Since rc8 that is the verdict first, then the last round's seat answers,
which the app folds under it. A build with no verdict gives the last round's
positions and the chair's last ruling.

`--tag b` writes a second run of the day as `results/<date>b.md`, and
`--baseline <date>` adds a before and after table against that day's
`summary.json`.

## Rubric

| Line | What earns a 5 |
|---|---|
| Surfaces the key risks | The risks that could hurt this person, not generic warnings |
| Names trade-offs | What each option costs and buys |
| Clear recommendation with confidence | One call the reader finds quickly, how sure, what would change it |
| Flags what is unknown | Which missing facts matter and why |
| Catches the hidden facts | Raises nearly all the hidden facts |
| Actionable next steps | Concrete, ordered steps to start today |
| Honest minority view | The strongest case against its own call, fairly |

## Run it

```bash
bench/run.sh --validate                    # check the scenarios; calls nothing
RT_BIN=roundtable bench/run.sh             # full run, today's date
bench/run.sh --only churn,restructure      # a subset
bench/run.sh --resume                      # reuse saved scenarios for the date
bench/run.sh --report-only                 # rebuild the .md from saved results
```

Defaults: seats `claude,codex,grok`, 2 rounds, single AI `claude`, judge
`opencode`, 3 scenarios at once. Override with `--seats`, `--rounds`,
`--single`, `--judge` (or `RT_SEATS`, `RT_ROUNDS`, `RT_SINGLE`, `RT_JUDGE`).
The runner refuses a judge that is also a seat.

Every call goes through the `roundtable` command (`RT_BIN`), so it uses the
same signed-in apps and keys a person would. It needs a build with `--events`
(the engine from `desktop/engine`). A full run is about 130 model calls: per
scenario up to 6 seat answers and 2 chair rulings, 1 single answer, 2 judge
calls.

## Reading the numbers

The judge is one model. Treat a gap under 0.5 on a line as noise. The bench
is a regression check across builds, not a proof that the council is right.
