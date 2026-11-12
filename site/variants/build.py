#!/usr/bin/env python3
"""Build the roundtable.sh marketing site design variants and the picker page.

Run: python3 site/variants/build.py
Writes a.html, b.html, c.html beside this file. index.html (the picker) is
written by hand and links the screenshots in shots/.

Every variant carries the same copy, so the only difference is the design.
Copy rules: plain words, no em dashes, buttons are a verb and an object.
"""
from pathlib import Path
import math

HERE = Path(__file__).parent

DOWNLOAD_MAC = "https://github.com/frontier-infra/roundtable/releases/download/desktop-latest/Roundtable.dmg"
DOWNLOAD_WIN = "https://github.com/frontier-infra/roundtable/releases/download/desktop-latest/Roundtable-Setup.exe"
RELEASES = "https://github.com/frontier-infra/roundtable/releases"
DOCS = "https://roundtable.sh/docs/get-started"
GITHUB = "https://github.com/frontier-infra/roundtable"

AIS = [
    ("claude", "Claude", "Anthropic"),
    ("openai", "ChatGPT", "OpenAI"),
    ("gemini", "Gemini", "Google"),
    ("grok", "Grok", "xAI"),
    ("glm", "GLM", "Z.AI"),
    ("minimax-icon", "MiniMax", "MiniMax"),
]

# A product UI mock: one question, four answers, the chair's verdict.
ANSWERS = [
    ("claude", "Claude", "Agrees", "SQLite. One tenant, one file, no server to run. Move to Postgres when you need many writers at once."),
    ("openai", "ChatGPT", "Agrees", "SQLite with WAL mode on. Plan the move to Postgres once writes pass a few per second."),
    ("gemini", "Gemini", "Agrees", "SQLite fits this load. Keep the schema portable so a later move stays cheap."),
    ("grok", "Grok", "Disagrees", "Postgres. Internal tools grow, and a migration later costs more than the setup now."),
]


def mark(size, ink, chair, ground=None):
    """The Roundtable mark: a round table with six seats, the top seat is the chair."""
    parts = []
    if ground:
        parts.append(f'<rect x="0" y="0" width="100" height="100" rx="22" fill="{ground}"/>')
    parts.append(f'<circle cx="50" cy="50" r="14" fill="none" stroke="{ink}" stroke-width="3.6"/>')
    for i in range(6):
        a = math.radians(-90 + i * 60)
        cx, cy = 50 + 26 * math.cos(a), 50 + 26 * math.sin(a)
        c = chair if i == 0 else ink
        parts.append(f'<circle cx="{cx:.2f}" cy="{cy:.2f}" r="2.8" fill="{c}"/>')
        # open arc behind the seat, facing the table
        r = 5.6
        a1, a2 = a - math.radians(70), a + math.radians(70)
        ox, oy = 50 + 31 * math.cos(a), 50 + 31 * math.sin(a)
        x1, y1 = cx + r * math.cos(a1), cy + r * math.sin(a1)
        x2, y2 = cx + r * math.cos(a2), cy + r * math.sin(a2)
        parts.append(f'<path d="M {x1:.2f} {y1:.2f} A {r} {r} 0 0 1 {x2:.2f} {y2:.2f}" fill="none" stroke="{c}" stroke-width="2.4" stroke-linecap="round"/>')
    return f'<svg class="mark" width="{size}" height="{size}" viewBox="0 0 100 100" aria-hidden="true">{"".join(parts)}</svg>'


def logo(file, size=28):
    return f'<img src="img/{file}.svg" width="{size}" height="{size}" alt="" loading="lazy">'


def app_frame():
    cards = "".join(
        f'''<article class="ans">
          <header>{logo(f, 22)}<b>{n}</b><span class="st {'no' if s == 'Disagrees' else 'ok'}">{s}</span></header>
          <p>{t}</p>
        </article>''' for f, n, s, t in ANSWERS)
    return f'''<div class="app-frame" role="img" aria-label="The Roundtable app showing four AI answers and the chair's verdict">
      <div class="app-bar"><i></i><i></i><i></i><span>Roundtable</span><em>Talk it through, round 2 of 4</em></div>
      <div class="app-body">
        <div class="app-q"><span class="mono">Question</span><p>Postgres or SQLite for a single-tenant internal tool?</p></div>
        <div class="ans-grid">{cards}</div>
        <div class="verdict">
          <div class="v-head">{logo("claude", 20)}<b>Chair verdict</b><span class="mono">CONSENSUS 3 of 4</span></div>
          <p>Use SQLite with WAL mode. Grok still expects growth, so move to Postgres if writes pass 10 a second.</p>
        </div>
      </div>
    </div>'''


def table_visual(v):
    seats = []
    for i, (f, n, _) in enumerate(AIS):
        a = -90 + i * 60
        seats.append(f'<li class="seat" style="--a:{a}deg;--i:{i}"><span>{logo(f, 34)}</span><b>{n}</b></li>')
    return f'''<div class="table-vis" data-loop>
      <div class="table-top">{mark(120, v["ink"], v["accent"])}<span class="mono t-label">Consensus</span></div>
      <ul class="seats">{"".join(seats)}</ul>
    </div>'''


def lanes_visual():
    rows = "".join(f'<li style="--i:{i}"><span>{logo(f, 26)}</span><b>{n}</b><i class="pulse"></i></li>' for i, (f, n, _) in enumerate(AIS))
    return f'''<div class="lanes" data-loop>
      <div class="lane-in"><span class="mono">Your question</span><p>Postgres or SQLite for an internal tool?</p></div>
      <ul class="lane-ais">{rows}</ul>
      <div class="lane-out"><span class="mono">Chair verdict</span><p>SQLite with WAL mode. 5 of 6 agree.</p></div>
    </div>'''


HERO_COPY = dict(
    h1="Ask all your AIs one question, at once",
    sub="Roundtable sends your question to Claude, ChatGPT, Gemini, Grok and more. Every answer lands side by side, and Claude, as chair, writes where they agree.",
    fact="Free and open source. Mac, Windows, Claude Code and the command line.",
)


def hero(v):
    btns = f'''<div class="actions">
        <a class="btn primary" href="#install">Download Roundtable</a>
        <a class="btn ghost" href="{DOCS}">Read the docs</a>
      </div>'''
    copy = f'''<h1 id="hero-h">{HERO_COPY["h1"]}</h1>
      <p class="lede">{HERO_COPY["sub"]}</p>
      {btns}
      <p class="fact mono">{HERO_COPY["fact"]}</p>'''
    if v["hero"] == "table":
        return f'''<section class="hero hero-table" aria-labelledby="hero-h"><div class="wrap">
      <div class="hero-copy">{copy}</div>
      {table_visual(v)}
    </div></section>'''
    if v["hero"] == "editorial":
        return f'''<section class="hero hero-ed" aria-labelledby="hero-h"><div class="wrap">
      <h1 id="hero-h">{HERO_COPY["h1"]}</h1>
      <div class="ed-row">
        <p class="lede">{HERO_COPY["sub"]}</p>
        <div>{btns}<p class="fact mono">{HERO_COPY["fact"]}</p></div>
      </div>
      <div class="bleed">{app_frame()}</div>
    </div></section>'''
    return f'''<section class="hero hero-lanes" aria-labelledby="hero-h"><div class="wrap">
      <div class="hero-copy center">{copy}</div>
      {lanes_visual()}
    </div></section>'''


def sections(v):
    show_app = v["hero"] != "editorial"
    app = f'''<section id="app" aria-labelledby="app-h" style="--h:1"><div class="wrap">
      <div class="sec-head"><h2 id="app-h">Every answer in one window</h2>
      <p>Each AI answers on its own card. The chair writes the verdict: what they agree on, and what they still dispute.</p></div>
      {app_frame()}
    </div></section>''' if show_app else f'''<section id="app" aria-labelledby="app-h" style="--h:1"><div class="wrap">
      <div class="sec-head"><h2 id="app-h">Every answer in one window</h2>
      <p>Each AI answers on its own card. The chair writes the verdict: what they agree on, and what they still dispute.</p></div>
      <div class="facts3">
        <div><span class="num">8</span><p>AI apps it finds signed in on your computer, such as Claude Code, Codex and Cursor</p></div>
        <div><span class="num">4</span><p>rounds at most in Talk it through, and it stops early once they agree</p></div>
        <div><span class="num">0</span><p>Roundtable accounts. Questions go straight to the AIs you connected</p></div>
      </div>
    </div></section>'''

    modes = f'''<section id="modes" aria-labelledby="modes-h" style="--h:1"><div class="wrap">
      <div class="sec-head"><h2 id="modes-h">2 ways to ask a question</h2>
      <p>Pick one round for fast, independent takes, or let the AIs read each other and revise.</p></div>
      <div class="cards2">
        <article class="card">
          <div class="mode-vis one">{''.join('<i></i>' for _ in range(5))}</div>
          <span class="mono">1 round</span>
          <h3>Quick answers</h3>
          <p>Each AI answers once, on its own, so the takes stay independent.</p>
        </article>
        <article class="card">
          <div class="mode-vis many">{''.join(f'<i style="--r:{r}"></i>' for r in range(4))}</div>
          <span class="mono">Up to 4 rounds</span>
          <h3>Talk it through</h3>
          <p>They read each other and revise until the chair says they agree.</p>
        </article>
      </div>
      <ul class="extras">
        <li>Let them search the web</li><li>Add a file or a link they read first</li><li>Have one AI sharpen your question first</li>
      </ul>
    </div></section>'''

    tiles = "".join(f'<li class="ai"><span>{logo(f, 40)}</span><b>{n}</b><small>{p}</small></li>' for f, n, p in AIS)
    ais = f'''<section id="ais" aria-labelledby="ais-h" style="--h:1"><div class="wrap">
      <div class="sec-head"><h2 id="ais-h">Works with the AI apps you already use</h2>
      <p>Roundtable first uses the AI apps signed in on your computer, on the plans you already pay for. No app? Paste an API key.</p></div>
      <ul class="ai-grid">{tiles}</ul>
      <p class="also mono">Also Claude Code, Codex, Gemini CLI, Qwen Code, Kimi, OpenCode, Cursor, OpenRouter, Ollama and LM Studio</p>
    </div></section>'''

    install = f'''<section id="install" aria-labelledby="install-h" style="--h:1"><div class="wrap">
      <div class="sec-head"><h2 id="install-h">Install Roundtable</h2>
      <p>Download the app, or use Roundtable in Claude Code or on the command line.</p></div>
      <div class="inst">
        <article class="card inst-app">
          <h3>The app for Mac and Windows</h3>
          <div class="actions">
            <a class="btn primary" href="{DOWNLOAD_MAC}">Download for Mac</a>
            <a class="btn ghost" href="{DOWNLOAD_WIN}">Download for Windows</a>
          </div>
          <p class="small">macOS 11 or later, Apple silicon or Intel. Windows 10 or 11, 64-bit. <a class="link" href="{RELEASES}">See all releases</a></p>
        </article>
        <article class="card">
          <h3>In Claude Code</h3>
          <pre><code>/plugin marketplace add frontier-infra/roundtable
/plugin install roundtable@frontier-infra</code></pre>
          <button class="copy" type="button">Copy command</button>
        </article>
        <article class="card">
          <h3>On the command line</h3>
          <pre><code>uv tool install roundtable-sh</code></pre>
          <button class="copy" type="button">Copy command</button>
        </article>
        <article class="card">
          <h3>With the script</h3>
          <pre><code>curl -fsSL https://roundtable.sh/install.sh | bash</code></pre>
          <button class="copy" type="button">Copy command</button>
        </article>
      </div>
    </div></section>'''

    close = f'''<section id="close" class="close" aria-labelledby="close-h" style="--h:1"><div class="wrap">
      {mark(88, v["ink"], v["accent"])}
      <h2 id="close-h">Roundtable is free and open source</h2>
      <p>MIT license. Runs on your own AI plans and keys. No Roundtable account.</p>
      <div class="actions center"><a class="btn primary" href="#install">Download Roundtable</a></div>
    </div></section>'''
    return app + modes + ais + install + close


def page(key, v):
    nav = f'''<header class="nav"><div class="wrap">
      <a class="brand" href="#top">{mark(28, v["ink"], v["accent"])}<span>Roundtable</span></a>
      <nav aria-label="Main"><a href="#app">Product</a><a href="#modes">Modes</a><a href="#ais">AIs</a><a href="{DOCS}">Docs</a><a href="{GITHUB}">GitHub</a></nav>
      <a class="btn primary sm" href="#install">Download Roundtable</a>
    </div></header>'''
    footer = f'''<footer><div class="wrap">
      <div class="f-brand">{mark(24, v["ink"], v["accent"])}<b>Roundtable</b><span>A Frontier Infra project, built by Titanium Computing.</span></div>
      <div class="f-links"><a href="{DOCS}">Docs</a><a href="{GITHUB}">GitHub</a><a href="{GITHUB}/issues">Support</a><a href="https://frontierinfra.org">Frontier Infra</a><a href="https://titaniumcomputing.com">Titanium Computing</a></div>
      <p class="credit">MIT License. Product names and logos are trademarks of their owners and name compatible services only. Logos: Gil Barbara, gilbarbara/logos, CC0.</p>
    </div></footer>'''
    return f'''<!doctype html>
<html lang="en" data-type="software" data-variant="{key}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>Roundtable: ask all your AIs one question, at once</title>
<meta name="description" content="Roundtable is a free, open-source app that asks Claude, ChatGPT, Gemini, Grok and more the same question and shows where they agree.">
<meta name="page-type" content="software"><meta name="site-kind" content="software">
<meta name="robots" content="noindex">
<link rel="icon" href="img/roundtable-mark.svg" type="image/svg+xml">
<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="{v["fonts"]}" rel="stylesheet">
<link rel="stylesheet" href="base.css">
<style>
/* Variant {key}: {v["name"]}. {v["note"]} */
:root {{ {v["tokens"]} }}
{v["css"]}
</style>
</head>
<body id="top">
{nav}
<main>
{hero(v)}
{sections(v)}
</main>
{footer}
<script src="site.js" defer></script>
</body>
</html>
'''


VARIANTS = {
    "a": dict(
        name="Chair pink",
        note="The current brand, done properly: near black, one pink chair, Geist, sharp corners. The table is the hero picture.",
        hero="table", ink="#f3f3f2", accent="#ff2e88",
        fonts="https://fonts.googleapis.com/css2?family=Geist:wght@400..600&family=Geist+Mono:wght@400..500&display=swap",
        tokens="--bg:#0b0b0c;--surface:#141415;--surface-2:#1c1c1e;--line:#29292c;--ink:#f3f3f2;--muted:#a5a5aa;--accent:#ff2e88;--accent-ink:#0b0b0c;--ok:#3ecf8e;--no:#f5b544;--tile:#f3f3f2;--r:6px;--r-lg:10px;--font-d:'Geist',system-ui,sans-serif;--font-b:'Geist',system-ui,sans-serif;--font-m:'Geist Mono',ui-monospace,monospace;--track:-0.045em;--h1:clamp(44px,5.4vw,84px);--hw:500",
        css="""
.hero-table .wrap{display:grid;grid-template-columns:1fr 1fr;gap:64px;align-items:center}
.hero{background:radial-gradient(60% 60% at 75% 50%,rgba(255,255,255,.04),transparent 70%)}
#app,#install{background:var(--surface)}
.close{border-top:1px solid var(--line)}
""",
    ),
    "b": dict(
        name="Cobalt paper",
        note="A light, editorial rebrand: warm paper, cobalt ink, Host Grotesk at large size, the app window as the hero picture.",
        hero="editorial", ink="#062b8a", accent="#e5484d",
        fonts="https://fonts.googleapis.com/css2?family=Host+Grotesk:wght@400..600&family=JetBrains+Mono:wght@400..500&display=swap",
        tokens="--bg:#efe7d8;--surface:#f7f2e8;--surface-2:#e6dbc7;--line:#d3c6ad;--ink:#062b8a;--muted:#3d4f86;--accent:#062b8a;--accent-ink:#f7f2e8;--ok:#13804f;--no:#b3261e;--tile:#fffdf8;--r:10px;--r-lg:14px;--font-d:'Host Grotesk',system-ui,sans-serif;--font-b:'Host Grotesk',system-ui,sans-serif;--font-m:'JetBrains Mono',ui-monospace,monospace;--track:-0.04em;--h1:clamp(48px,7.6vw,128px);--hw:500",
        css="""
.hero-ed h1{max-width:13ch}
.hero-ed .ed-row{display:grid;grid-template-columns:7fr 5fr;gap:48px;align-items:end;margin-top:28px}
.hero-ed .bleed{margin-top:48px}
.hero-ed .app-frame{box-shadow:0 30px 60px -30px rgba(6,43,138,.35)}
main>section.hero{min-height:auto;padding-bottom:0;overflow:hidden;max-height:calc(100svh - 64px);align-items:flex-start}
@media (max-width:720px){main>section.hero{max-height:none;padding-bottom:40px}.hero-ed .app-q{flex-direction:column;gap:4px}}
#modes,#install{background:var(--surface)}
.sec-head h2{font-size:clamp(36px,4.4vw,64px)}
.card{background:var(--tile)}
.nav{background:color-mix(in srgb,var(--bg) 88%,transparent)}
""",
    ),
    "c": dict(
        name="Forest lime",
        note="A new, warmer identity: deep forest green, lime chair, Urbanist with DM Mono, round corners. The hero shows one question fanned out to every AI.",
        hero="lanes", ink="#ffffff", accent="#c6f19d",
        fonts="https://fonts.googleapis.com/css2?family=Urbanist:wght@400..600&family=DM+Mono:wght@400;500&display=swap",
        tokens="--bg:#0f1f10;--surface:#162a17;--surface-2:#1d351e;--line:#2a452b;--ink:#ffffff;--muted:#b4c7b2;--accent:#c6f19d;--accent-ink:#0f1f10;--ok:#c6f19d;--no:#ffb27a;--tile:#f4f8ef;--r:14px;--r-lg:22px;--font-d:'Urbanist',system-ui,sans-serif;--font-b:'Urbanist',system-ui,sans-serif;--font-m:'DM Mono',ui-monospace,monospace;--track:-0.04em;--h1:clamp(40px,4.8vw,72px);--hw:500",
        css="""
.btn{border-radius:999px}
.hero-lanes{padding-block:40px!important}
.hero-lanes .wrap{display:flex;flex-direction:column;align-items:center;gap:36px}
.hero-lanes .lede{margin:18px auto 24px}
.hero-lanes .fact{margin-top:16px}
.lane-ais{gap:7px}
.lane-ais li{padding-block:5px}
.lane-ais li span{width:30px;height:30px}
.lane-ais li span img{width:22px;height:22px}
.hero-lanes h1{max-width:16ch;margin-inline:auto}
.hero-lanes .lede{margin-inline:auto}
#modes,#install{background:var(--surface)}
""",
    ),
}

if __name__ == "__main__":
    for key, v in VARIANTS.items():
        (HERE / f"{key}.html").write_text(page(key, v))
        print("wrote", key + ".html")
