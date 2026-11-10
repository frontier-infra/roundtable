# Decision model hook (for the verdict lane)

The decision model rates the table's verdict after the final round. Everything lives
in `engine/src/decision/`. The verdict step owns one call.

## The call

```rust
let decision = crate::decision::after_verdict(
    &crate::decision::Verdict {
        question: &opts.question,
        context: opts.context.as_deref(),
        positions: &finals,            // Vec<(seat label, final answer)>, the rc7 `finals`
        chair: final_chair.as_deref(), // the rc7 final verdict text, else the last round's chair
        consensus,                     // Option<bool>; None in advisory mode
    },
    keys,
    settings,
    opts.timeout,
);
if let Some(d) = &decision {
    out.push('\n');
    out.push_str(&d.markdown());        // the transcript section, so history and the .md export carry it
    emit(&Event::Decision(d.clone()));  // the app's `decision` event
}
```

This sits at the end of `council::run`, after the advisory or deliberation branch.
`say_verdict` (the rc7 final verdict) now returns its text, which becomes `final_chair`;
without a verdict shape, the last round's chair text is used. If the verdict step
moves again, keep three things:

1. Call `after_verdict` once, after the last round and the final verdict.
2. Pass the final positions, not round 1.
3. Append `markdown()` to the transcript and emit `Event::Decision`.

`after_verdict` returns `None` when the setting is off (the default), so it costs
nothing then. It never panics. Each network call is bounded by a timeout.

## What it does

1. `options::ChairOptions` asks the chair (Claude, by the council's usual route) to
   list the distinct options the seats argued (2 to 6) and which one the table settled on.
   If the chair cannot be asked, each answered seat's opening line becomes an option.
   "Need more information" (`more_information`) is always last.
2. `provider::resolve` builds the chosen provider. All three speak `POST /v1/systemone`:
   - `jev`: `https://api.typesafe.ai`, model `jev-latest`, key from `TYPESAFE_API_KEY`,
     `JEV_API_KEY` or the keychain account `typesafe`.
   - `jebadiah`: the saved address, `ROUNDTABLE_JEBADIAH_URL`, or a local server found by
     `detect::find_jebadiah` (`jeb serve` on 127.0.0.1:8100, AINode on :3000 or :8000).
     Only our builds count (`detect::is_ours`): `frontier-infra/jebadiah-*` (also under
     `hf.co/`) and the Ollama names `frontierinfra/jebadiah` and `jbrashearfrontierinfra/jebadiah`.
     Any other model named Jebadiah is skipped, with one line in the detect `note`.
     Optional key: `JEBADIAH_API_KEY`, `AINODE_API_KEY` or the keychain account `jebadiah`.
   - `systemone`: any other System One model (the LINK21 slot): saved name, address and model id.
3. One choice question goes to the model. The answer becomes `Outcome::Rated(Report)`
   with the verdict line, probabilities per option, confidence, and a plain disagreement
   sentence when the model's top option is not the table's pick.

## Surfaces

- Setting: `settings.decision = {provider, url, model, name}`; `provider: null` is off.
- CLI: `roundtable ask ... --decision jev|jebadiah|systemone|link21|off`;
  `roundtable decision status|detect|test <provider>`;
  `roundtable auth set jev` (key on stdin) saves the Jev key, the same way the app does.
- MCP: the `roundtable` tool takes `decision` (same values).
- App: `decision` event, shown under the final verdict (`ui/src/decision.ts`),
  a "Decision model" section in Settings, and `decisionMarkdown` in `runMarkdown`
  (Copy as Markdown, Download .md, session export).
