# What's new in Roundtable

Every version of the Roundtable app for Mac and Windows, newest first.
Each release adds a section here. The app, the GitHub release and roundtable.sh/changelog all show it.

## 0.1.2 - 2026-11-10

### Added

- Roundtable finds the AI apps already on your computer, such as Claude Code, Codex and Gemini CLI. It uses the plans you are signed in to. An API key is only a backup.
- Personas. Give each seat a role, such as CFO, Head of Sales or Security reviewer. Packs cover business roles, expert panels and simulated views of the big AI labs. You can write your own.
- Rounds. Choose 1 to 4 rounds. In later rounds each AI reads the others and changes its mind or holds its ground.
- The question coach. One AI asks up to three short questions, then writes a brief for the table. You can edit it or skip it.
- A suggested panel. From your brief, Roundtable proposes 2 to 4 seats. Keep them, change them or use your own.
- Blind first round. In round 1 each AI answers without seeing the others, so nobody copies the first answer.
- Verdict shapes: a decision with confidence, pros and cons, an action plan, or a vote. Every verdict names the minority view.
- Follow-ups. Ask the whole table or one seat a follow-up question. The conversation goes with it.
- Files and links. Attach a text file, a PDF or a web page for the table to read.
- A decision model can rate the verdict: Jev from TypeSafe, or Jebadiah running on your own computer. Settings shows how to get Jebadiah.
- Other providers. Add OpenRouter, Ollama, LM Studio, Groq, Together or any compatible service. Each model you tick gets its own seat.
- Updates. Roundtable checks for a new version when it opens and every 6 hours, and shows what changed. Settings, Updates has the switches.
- What's new. This list, in the app after an update and from Settings.

### Improved

- A new look for every screen, in Roundtable's own colors, with each AI's real logo.
- The verdict comes first: one decision, how sure the table is, what would change it, and three next steps.
- Verdicts are Markdown. Copy one or save it as a .md file. Export a whole session as one file.
- Real debate. After each round the chair names the leading pick and the open differences, and the next round argues them.
- When the answer depends on facts you did not give, the chair asks first.
- Each AI answers in a clean, private space. It cannot see your files or run commands.
- Live cards show each AI as it works, with one fix button when something goes wrong.
- Settings, About says who made Roundtable. Saved verdicts end with a short credit line you can turn off.

### Fixed

- The question coach and the chair now read the files and links you attach.
- Grok no longer stops early with "max turns reached".
- An AI that fails or runs out of time is asked once more, unless it is signed out or out of credit.
- A slow network on Windows now shows as a time-out, not a connection error.
- Every version now has its own number, so you can tell test builds apart.

## 0.1.1 - 2026-11-09

Test builds only. This version was never released.

### Added

- The first Roundtable app for Mac and Windows. Ask Claude, ChatGPT, Gemini and others one question, see each answer side by side, and get one clear verdict.
- Connect Roundtable to the Claude app and Claude Code.
- Past roundtables stay on your computer. Nothing is sent to us.
