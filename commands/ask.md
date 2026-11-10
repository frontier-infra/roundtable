---
description: Ask the Roundtable council a question and get one decision back
argument-hint: <question>
---

Convene the Roundtable council on this question: $ARGUMENTS

Follow the roundtable skill. Use the `roundtable` MCP tool if it is available. Otherwise run
`roundtable ask -q "<question>"` in the Bash tool. Quick is the default. If the question is
complex and high stakes (several stakeholders, money or legal stakes, more than three options,
missing facts), use Deep Think (MCP `mode: "deep"`, or `--mode deep`); it takes about 14 minutes. If it needs named expertise, seat a preset with `--personas` (for
example `pricing`, `security-review` or `exec-team`); run `roundtable personas` to see them. If the question is vague, use the coach first (MCP `coach: true`,
or `--coach-questions`, then `--coach-answer` once per answer). A table gets a decision verdict by default (or ask for
`pros-cons`, `plan`, `vote`); lead with it, and with its questions when it opens with `QUESTIONS FIRST`, and name its minority view. Return the question recap, the results table, and the judge's decision.
