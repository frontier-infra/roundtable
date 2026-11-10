# Persona packs

A persona is a seat role at the table. It has a name, a one-line expertise, a short brief
(how the seat reasons, what it weighs, what it doubts) and an optional preferred AI.
The persona sits on an AI seat. Without a persona a seat is Generic, as before.

Each file here is one pack. Edit a file, or copy one into your own personas folder:

- Desktop app: Settings, Personas (saved in the app data folder, `personas/`).
- Command line: `~/.config/roundtable/personas/*.json`, or `ROUNDTABLE_PERSONAS_DIR`.
- The desktop app's folder is read by the command line too, so your app personas work there.

## Shape

```json
{
  "id": "my-pack",
  "name": "My pack",
  "personas": [
    {"id": "cfo", "name": "CFO", "expertise": "Money, risk and return",
     "brief": "Reason in numbers ...", "ai": "claude"}
  ],
  "presets": [
    {"id": "money-panel", "name": "Money panel", "seats": ["cfo", "skeptical-investor"]}
  ]
}
```

`ai` is optional: one of `claude, openai, grok, glm, minimax, gemini, codex, qwen, kimi,
opencode, cursor`. A preset may name personas from any pack.

## Use it

```bash
roundtable ask -q "Should we raise prices?" --personas pricing
roundtable ask -q "..." --personas cfo,ceo@claude
roundtable ask -q "..." --personas generic
```

## The Frontier labs pack

Seats are named for a lab's public view, for example "The NVIDIA view". Each brief describes
a reasoning style drawn from public, widely reported positions. It names no person and never
speaks for the lab. Every lab seat shows this label in the app and in transcripts:

> Simulated perspective drawn from public positions. Not affiliated with or endorsed by NVIDIA.

One switch hides the pack: the Personas setting "Show the Frontier labs pack", the
`ROUNDTABLE_HIDE_LAB_PACK=1` environment variable, or a desktop build without the
`lab-pack` feature.
