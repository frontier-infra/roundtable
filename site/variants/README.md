# roundtable.sh design variants

Three directions for the public marketing site at roundtable.sh, for Jason to pick one.
Open `index.html` (the picker), then `a.html`, `b.html` and `c.html`.

- Built with the taste skill (webdevtodayjason/taste-skill): software page, one section per screen,
  real AI logos in their own colours, the product UI as live HTML, hover and press states, no scroll effects.
- Palettes and type pairs for B and C came from the skill's dealer; A uses the brand file of the desktop app.
- `python3 build.py` rebuilds the three pages from the shared copy.
- Nothing here is deployed. The live site source is frontier-infra/roundtable.sh; the picked variant gets built there.
