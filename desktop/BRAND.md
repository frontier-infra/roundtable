# Roundtable brand, desktop app

The desktop app uses the same identity as roundtable.sh and the site repo
(`frontier-infra/roundtable.sh`). The site's brand is thin: a mark, one accent
and two typefaces. This file turns it into one system for the app. Every value
below lives as a token at the top of `ui/src/styles.css`.

## The mark

- A round table with six seats, as on roundtable.sh (`static/img/logo.svg`).
- Each seat is a dot with an open arc behind it, facing the table.
- The app draws it as clean vector code in `ui/src/brand.ts` (`mark()`), not as
  a traced bitmap.
- The top seat is the chair. It is the only part in Roundtable pink. Claude
  chairs the "Talk it through" mode, so the colour carries meaning.
- The lockup is the mark plus the word "Roundtable" in Inter, weight 680,
  letter spacing -0.02em.
- The app icon is the mark in white on a near-black rounded square, chair in
  pink. Source: `core/icons/icon.svg`.

## Colour

Dark is the main theme, as on the site. Light follows the system setting.

| Token | Dark | Light | Use |
|---|---|---|---|
| `--bg` | #0b0b0c | #f6f6f4 | Window ground |
| `--surface` | #141415 | #ffffff | Rail, sidebar, cards |
| `--surface-2` | #1c1c1e | #efefec | Hover, notices |
| `--line` | #29292c | #e2e2de | Hairlines, card edges |
| `--ink` | #f3f3f2 | #131314 | Text |
| `--muted` | #a5a5aa | #5d5d63 | Second-level text |
| `--faint` | #8b8b91 | #6e6e74 | Times, versions, step numbers |
| `--accent` | #ff2e88 | #c2185b | Roundtable pink |
| `--ok` | #3ecf8e | #13804f | Connected, answered, agreed |
| `--bad` | #ff7a72 | #b3261e | Errors |
| `--warn` | #f5b544 | #9a5b00 | Not ready, still disagree |

- Pink is the site's accent (#ff2e88 dark, #c2185b light). Text on pink is
  near-black in dark mode and white in light mode, so it passes WCAG AA.
- Pink marks two things only: the one main action on a screen, and what is
  chosen (the current step, a ticked app, the picked mode, a switch that is on).
  A pink button inside a card turns to outline, so a screen has one pink button.
- Greys are neutral. No violet or lavender anywhere in the app's own design.
- No gradients, glows or tinted panels.

## Type

- Inter (variable) for every word. JetBrains Mono (variable) for small facts:
  versions, times, step numbers, model names, keys and command lines. Both
  typefaces are on the site.
- Both are bundled in `ui/src/assets/fonts` (SIL Open Font License). The app's
  content security policy loads fonts from the app only.
- Scale: 44 welcome title, 30 screen title, 28 ask title, 22 question title,
  16 section title, 14.5 body, 13 small, 12 mono.

## Shape and space

- Radius 8 for buttons and fields, 12 for cards and panels.
- One card style for an AI, an app found, a Claude connection and an answer:
  surface, 1px line, radius 12, padding 16. Cards never sit inside cards.
  Settings sections are rows split by hairlines, not cards.
- Equal edges: the rail and sidebar pad 28 and 14; the main column pads 32
  (56 in the first-run screens) on both sides, and the top bar uses the same
  lines as the content under it.
- Status is a word led by a small dot ("Answered", "Connected"). No pills.

## AI logos

- Each AI shows its own logo, in its own colours, on a light tile so dark
  marks and colour marks both read on the dark ground.
- Files in `ui/src/assets/logos`, from Gil Barbara's logos set (CC0), fetched
  with the tasteskill icon resolver. `CREDITS.json` lists source and licence.
- Every mark is its owner's trademark, used only to name the AI it stands for.
- Interface glyphs (settings, search, back, check, file) are Lucide (ISC),
  drawn inline in `brand.ts`.

## The table, as the picture

The first-run rail shows the table large. Seats fill with the logos of the AIs
connected so far. The welcome screen seats the six AIs Roundtable can ask.
This is the app's one picture; it shows the product's idea, not a mood.

## Motion

- Buttons lift 1px on hover and press on click.
- The thinking dot pulses; spinners turn while work runs.
- Under reduced motion, nothing moves.

## Voice

Plain consumer words, as on the site's welcome line: "Ask several AIs at once.
Get one clear answer." Verbs on buttons ("Ask the table", "Connect to Claude
Code"). Short sentences. No jargon on the first-run screens.
