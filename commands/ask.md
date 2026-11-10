---
description: Ask the Roundtable council a question and get one decision back
argument-hint: <question>
---

Convene the Roundtable council on this question: $ARGUMENTS

Follow the roundtable skill. Use the `roundtable` MCP tool if it is available. Otherwise run
`roundtable ask -q "<question>"` in the Bash tool. If the question is high stakes, use 3
rounds (at most 4). If it needs named expertise, seat a preset with `--personas` (for
example `pricing`, `security-review` or `exec-team`); run `roundtable personas` to see them. If the question is vague, use the coach first (MCP `coach: true`,
or `--coach-questions`, then `--coach-answer` once per answer). Ask for a verdict (`--verdict decision`, or
`pros-cons`, `plan`, `vote`) and name its minority view. Return the question recap, the results table, and the judge's decision.
