# Roundtable Desktop: spec

Status: built in step 2 on this branch. `desktop/README.md` says how to build and test it.
Branch: `feat/desktop-app` in `frontier-infra/roundtable`. Everything lives under `desktop/`.
Stack: Tauri 2, Rust core, TypeScript UI (tauri-standard).

## 1. What Roundtable does, in one paragraph

Roundtable asks several AI models the same question at once.
Each one answers on its own. That is "quick answers" (advisory mode).
It can also let them read each other and revise.
Then Claude acts as chair and says when they agree (deliberation mode).
Today it is a terminal tool: a bash engine (`lib/core.sh`), a Python MCP server (`lib/mcp.py`), and a `curl | bash` installer.
It needs bash, python3 and curl. Keys sit in a plain text file (`~/.config/roundtable/config.env`).
None of that works for a consumer. Stock Windows has no bash. Most people have never opened a terminal.

## 2. Who this is for

- A person who uses Claude (the Claude app, or Claude Code) and pays for at least one AI service.
- They can copy and paste. They can click a link and sign in to a website.
- They have never used a terminal and do not want to.
- They want a second opinion before a big choice: a purchase, a plan, a piece of writing, a code change.
- They are on a Mac (Apple silicon or Intel) or Windows 10/11.

What they must never see: a terminal, a file path to edit, an environment variable, the word "MCP".

## 3. Principles

1. One button per step. No step asks the person to type a command.
2. Keys live in the OS keychain. Never in a file, a log, or on screen after saving.
3. Plain words. 20 words a sentence at most. Say what happens next.
4. Every error says what to do, in one line, with a button to do it.
5. The app brings its own engine. Nothing to install first.
6. Honest about cost. Each provider bills the person for every answer.

## 4. First run: five screens

A progress dot row shows "Step 2 of 5". Every screen has Back. Screens 3 to 5 have "Skip for now".

### Screen 1. Welcome

- Title: **Ask several AIs at once. Get one clear answer.**
- Three short lines:
  - "Roundtable asks Claude, ChatGPT, Gemini and others the same question."
  - "You see each answer, side by side."
  - "Everything runs on this computer, with your own accounts."
- Small print: "Each AI company charges your account for the answers. Roundtable itself is free."
- Button: **Get started**

### Screen 2. Set up Roundtable

- Title: **Set up Roundtable**
- One line: "This takes a few seconds. You do not need to install anything else."
- Button: **Set up**
- While it works, three lines tick green in turn:
  - "Putting Roundtable in place"
  - "Checking it runs"
  - "Ready"
- Section 7.1 says what the button does. The person never sees a path.
- If it fails: "Setup did not finish. Click Try again. If it fails twice, click Get help."
- Buttons: **Try again**, **Get help** (opens the help page with the error code filled in).

### Screen 3. Connect your AI accounts

- Title: **Connect at least one AI**
- One line: "Two or more make a real roundtable. You can add more later."
- Two cards show first. The rest sit under **More AIs**.

| Card | Provider | Key name the engine uses | "Get a key" opens |
|---|---|---|---|
| Claude | Anthropic | `ANTHROPIC_API_KEY` | console.anthropic.com/settings/keys |
| Gemini | Google | `GEMINI_API_KEY` | aistudio.google.com/apikey |
| ChatGPT (More AIs) | OpenAI | `OPENAI_API_KEY` | platform.openai.com/api-keys |
| Grok (More AIs) | xAI | `XAI_API_KEY` | console.x.ai |
| GLM (More AIs) | Z.ai | `ZAI_API_KEY` | the Z.ai API key page |
| MiniMax (More AIs) | MiniMax | `MINIMAX_API_KEY` | the MiniMax platform API key page |

A build script checks every link. A dead link fails the build.

- Each card has:
  - A field: "Paste your key here". The text shows as dots.
  - A link: **Where do I get a key?** It opens the provider page in the browser.
  - A three-step tooltip: "Sign in. Click Create key. Copy it and paste it here."
  - On paste, the app checks the key at once with one tiny request.
  - Green result: "Connected. Saved in your Mac's Keychain." On Windows: "Saved in Windows Credential Manager."
  - Red result: "That key did not work. Check you copied all of it."
  - Or: "That account has no credit left. Add credit on the provider's site."
- Claude's card carries a tag: **Recommended. Claude also acts as the chair.**
- Footer note: "A Claude or ChatGPT chat plan is not an API key. Use the link to get a key."
- Button: **Continue** (enabled once one card shows Connected).

### Screen 4. Connect to Claude

- Title: **Use Roundtable from inside Claude**
- One line: "Then you can tell Claude 'ask the roundtable', and it will."
- The app looks for Claude on this computer and shows what it found:
  - **Claude app found.** Button: **Connect to the Claude app**. After: "Done. Quit and reopen Claude to finish."
  - **Claude Code found.** Button: **Connect to Claude Code**. After: "Done. Start a new Claude Code session to use it."
  - Both can be connected. Each shows a green check when done.
- When neither is found:
  - "Claude is not on this computer yet." Link: **Download Claude** (claude.ai/download).
  - "Using Claude Code somewhere else? Paste these two lines into Claude Code:"
  - A box with a **Copy** button:
    ```
    /plugin marketplace add frontier-infra/roundtable
    /plugin install roundtable@frontier-infra
    ```
  - The names come from `.claude-plugin/marketplace.json` at build time. A test fails if they drift.
- Button: **Continue**, or **Skip for now**.

### Screen 5. Try your first roundtable

- Title: **Try it now**
- The question box is filled in: "I have a free Saturday. Should I learn to bake bread or start a vegetable garden?"
- The person can change it. Button: **Ask the table**.
- One card per connected AI appears. Each shows "Thinking..." and then its answer.
- When all are back: "That's it. You're set up." Button: **Open Roundtable** (goes to the main screen).
- If every AI fails: the plain error from the first failure, with **Back to keys**.

## 5. Main screen

Three areas. The window opens at 1100 by 720 and works down to 800 by 600.

- **Left: Past roundtables.**
  - Newest first. Each row shows the question's first line and the date.
  - A small Claude badge marks runs that Claude started.
  - A search box on top. Right-click a row to delete it.
- **Centre: Ask.**
  - A large question box: "What do you want to decide?"
  - Two large choice buttons:
    - **Quick answers**: each AI answers once, on its own. (Engine: `--rounds 1`.)
    - **Talk it through**: they read each other and revise. Claude, as chair, says when they agree. Up to 3 rounds. (Engine: `--rounds 3`.)
  - A switch: **Let them search the web** (engine `--research`). Off by default.
  - **Add a file** (optional): background the AIs should read (engine `--context`). Plain text in v1.
  - A line under the button: "This will ask 3 AIs, about 3 requests." It updates with the choices.
  - For Talk it through, that line shows the most requests it could make.
  - Button: **Ask the table**. While it runs, it becomes **Stop**.
- **Results** replace the ask box while a roundtable runs:
  - With Talk it through, the chair's verdict sits on top: "They agree" or "They still disagree".
  - Under the verdict: the agreed points and the open points.
  - Below, one card per AI: its name, its answer, and a status (answered, skipped, timed out).
  - Buttons: **Copy all**, **Save as file**, **Ask a follow-up** (a new roundtable with this one as background).
- **Top right:** a gear for Settings.
  - Next to it, a dot. Green means the engine runs and at least one AI is ready.

### Settings

- **AI accounts:** the six cards from screen 3. Replace or remove a key. A live status line per AI.
- **Who sits at the table:** a switch per connected AI. Saved as the default list.
- **Claude connection:** the two buttons from screen 4, plus **Disconnect**.
- **History:**
  - A switch: "Keep past roundtables on this computer". On by default (see Open questions).
  - **Delete all history**.
- **Updates:** current version, **Check now**, and "Install updates automatically" (on by default).
- **Advanced** (closed by default):
  - Model per AI. Defaults match `lib/core.sh`.
  - Time limit per AI. Default 300 seconds, 360 with web search.
  - **Use Roundtable in Terminal too**: puts a `roundtable` command on the PATH. Off by default.
  - **Import keys from the command-line version**: shown only when the CLI key file exists.
  - Import moves keys into the keychain. It asks before it removes them from the old file.
- **Help:**
  - A link to the roundtable.sh help pages and a link to report a problem.
  - The version.
  - **Copy diagnostics**: versions, OS and AI status. Never keys.

## 6. Decisions taken in this spec

Each is reversible. Each lists the options, the pick, the reason and the undo step.

### D1. Which engine the app runs

- Options:
  1. Ship the bash engine plus a bundled Python and bash. Windows needs Git Bash or busybox. Large and fragile.
  2. Port the engine to Python only and bundle a standalone Python (about 30 MB). That touches `lib/`, which another workflow owns.
  3. **Picked (recommended):** port the engine to Rust as a crate in `desktop/`. Build it as one `roundtable` binary.
- The Rust engine runs the same council and takes the same flags. It prints the same transcript and serves the same MCP tool.
- Reason: no runtime to install on either OS. One signed binary. The tauri-standard rule says Rust core. The engine is about 360 lines of HTTP calls and a chair loop.
- Cost: two engines until they converge. A parity test runs both against a fake provider server and diffs the transcripts.
- Undo: swap the sidecar for option 2. The app talks to the engine only through its command line, so the UI does not change.

### D2. Where keys live and who reads them

- Picked: the engine binary owns the keychain items. The app never reads a key back.
  - The app saves a key by running `roundtable auth set <provider>` with the key on stdin.
  - Mac: login Keychain, service `sh.roundtable.engine`, account = provider name.
  - Windows: Credential Manager, same names. Both through the Rust `keyring` crate.
- Reason: on Mac, a keychain item trusts the program that made it.
  - If the app wrote the item, Claude starting the engine would raise a password prompt.
  - One engine signed with a stable Developer ID never prompts, across updates too.
- Engine read order:
  1. Process environment (for example, keys the Claude Code plugin passes in).
  2. The OS keychain.
  3. The CLI key file `~/.config/roundtable/config.env`, read only, for people coming from the CLI.
- Undo: the app writes the items itself and adds the engine to each item's trusted apps.

### D3. How Claude is connected

- Picked: the app registers its own engine as an MCP server named `roundtable`.
  - Claude app: merge `mcpServers.roundtable = { command: <engine path>, args: ["mcp", "serve"] }` into `claude_desktop_config.json`.
  - It backs up the file first and never touches other entries. Invalid JSON is left alone, with a plain error.
  - Claude Code: run `claude mcp add roundtable --scope user -- <engine path> mcp serve`.
  - If that fails, merge the same block into `~/.claude.json`, as `lib/install.sh` does.
  - When Claude Code is absent: show the two plugin lines (screen 4).
- Reason: consumers mostly use the Claude app, not Claude Code. Covering both costs one JSON merge.
- Pointing Claude at the app's engine keeps keys in the keychain and works on Windows.
- The plugin's own MCP block runs the bash engine and asks for keys a second time.
- If the plugin is already installed, the app says so. It offers **Use the app's engine instead**.
  - That runs `claude plugin disable roundtable@frontier-infra`, so Claude does not see two `roundtable` tools.
- Undo: Settings > Claude connection > **Disconnect**. It removes only the `roundtable` entries.

### D4. Installer format

- Mac: one universal `.dmg` (Apple silicon and Intel). Drag to Applications.
  - Signed with Developer ID, notarized and stapled.
- Windows: NSIS `.exe`, per-user install, no admin prompt.
  - It installs WebView2 if missing (Tauri's download bootstrapper). Signed with Azure Trusted Signing.
- No MSI, Mac App Store or Microsoft Store in v1.
- Undo: add MSI or store builds later from the same Tauri config.

### D5. Versioning

- The app starts at 0.1.1, the same number as the CLI release, and climbs the patch digit to 99 (Jason's rule). Tags: `desktop-v0.1.N`.
- The engine binary reports the same version as the app it ships in.
- CLI tags (`v0.1.N`) and desktop tags never share a release.

## 7. Under the hood

### 7.1 Setting up the engine (screen 2)

The engine ships inside the app as a Tauri sidecar (`externalBin`) named `roundtable-engine`. The **Set up** button:

1. Copies the engine to a stable per-user place, so Claude's config survives if the app moves:
   - Mac: `~/Library/Application Support/sh.roundtable.app/engine/roundtable`
   - Windows: `%LOCALAPPDATA%\sh.roundtable.app\engine\roundtable.exe`
2. Runs `roundtable version --json` and checks it matches the app.
3. Runs `roundtable mcp selftest` (initialize and tools/list, no network).
4. Writes nothing else. No PATH change unless the person turns on "Use Roundtable in Terminal too".

After an update, the app re-copies the engine on launch when the version differs.
Claude picks up the new engine the next time it starts.

### 7.2 Engine command surface

The Rust engine keeps the CLI the README documents. It adds machine-readable modes for the app.

| Command | Used by | Notes |
|---|---|---|
| `roundtable ask -q ... [--heads] [--rounds] [--research] [--context] [--timeout] [--out]` | app, Terminal | Same flags and markdown transcript as `lib/core.sh`. |
| `roundtable ask ... --events` | app | One JSON event per line: `run_started`, `head_started`, `head_done`, `round_done`, `chair_verdict`, `run_done`. Drives the live cards. |
| `roundtable mcp serve` | Claude | Same single `roundtable` tool and schema as `lib/mcp.py`: question, heads, rounds, research. Returns the markdown transcript. |
| `roundtable auth set <provider>` (key on stdin), `auth remove`, `auth status --json` | app | Keychain only. Status never prints a key. |
| `roundtable doctor --json` | app | Per AI: has key, reachable, error class. Feeds the green dot and the key check. |
| `roundtable version --json` | app | Version and build. |

Heads in the app: claude, openai, gemini, grok, glm, minimax.
The `codex` head and the Grok CLI and Hermes fallbacks need local tools.
The Rust engine keeps them, but the app hides them.

### 7.3 History

- Each finished roundtable writes one Markdown transcript and one small JSON index row.
  - Mac: `~/Library/Application Support/sh.roundtable.app/history/`
  - Windows: `%APPDATA%\sh.roundtable.app\history\`
- Runs that Claude starts through MCP write there too, marked `source: "claude"`.
- So the person can see and reopen what Claude asked.
- Nothing leaves the computer. The Settings switch turns writing off. **Delete all history** removes the folder.

### 7.4 Settings storage

- `settings.json` in the same app data folder.
- It holds default heads, model overrides, time limits, the history switch and the update choice.
- No keys, ever. A test scans every file the app writes for key patterns. A match fails the build.

### 7.5 Privacy

- Same promise as the README.
- Questions and files go straight from this computer to the chosen providers.
- No Frontier Infra server sees them. No analytics.
- The updater fetches only the release manifest and installer from GitHub.

## 8. Mac and Windows differences

| Topic | Mac | Windows |
|---|---|---|
| Installer | Universal `.dmg`, drag to Applications | Per-user NSIS `.exe`, no admin |
| Signing | Developer ID (vault item "Apple Developer"), notarized | Azure Trusted Signing (the vault item for `titanium-gh-signing`) |
| Web view | WKWebView, built in | WebView2, installed by the bootstrapper if missing |
| Key store | Login Keychain | Credential Manager |
| Claude app config | `~/Library/Application Support/Claude/claude_desktop_config.json` | `%APPDATA%\Claude\claude_desktop_config.json`; also check the Microsoft Store package path |
| Claude Code detection | `claude` on PATH, `~/.local/bin/claude`, or `~/.claude.json` | `claude.exe` on PATH or `%USERPROFILE%\.claude.json` |
| Engine location | `~/Library/Application Support/sh.roundtable.app/engine/` | `%LOCALAPPDATA%\sh.roundtable.app\engine\` |
| Terminal command (opt-in) | Symlink in `~/.local/bin`, only if no `roundtable` is there already | Adds the engine folder to the user PATH |
| First open | Gatekeeper passes, because it is notarized | SmartScreen may warn until the signature builds reputation. A help line covers "More info, Run anyway". |
| Window | Native traffic lights and menu bar | Standard title bar |

## 9. Updates

- Tauri updater plugin, with signed manifests on GitHub Releases.
- Each release `desktop-v0.1.N` carries the installers, their `.sig` files and `latest.json`.
- CI also copies `latest.json` to a rolling release tagged `desktop-latest`. The app's endpoint is:
  `https://github.com/frontier-infra/roundtable/releases/download/desktop-latest/latest.json`
- That way a CLI release never looks like a desktop update.
- A Tauri updater key signs the manifests.
  - New vault item: "Roundtable Desktop updater key". Private key and password as fields.
  - Same shape as "Titanium Bot desktop updater key". The public key sits in `tauri.conf.json`.
- The app checks at launch and every 6 hours. It downloads in the background.
- Then a quiet banner: "An update is ready." Buttons: **Restart now**, **Later**.
- **Later** installs the update on the next quit.
- The engine updates with the app (7.1). There is no separate engine update.

## 10. Build and release

- Layout under `desktop/`:
  - `core/`: the Tauri 2 app crate. Sidecar, updater, Claude wiring.
  - `engine/`: the `roundtable` Rust binary. Council, chair, MCP server, auth, doctor.
  - `ui/`: the TypeScript UI. Svelte or Solid, nothing heavy.
  - `scripts/`: link check, parity test, release.
- CI, pull requests (a fork can trigger these):
  - Build and test engine and UI on Blacksmith only: `blacksmith-4vcpu-ubuntu-2404` and `blacksmith-4vcpu-windows-2025`.
  - No secrets in these jobs.
- CI, releases (a `desktop-v*` tag pushed on the main repo, or a manual run by a maintainer):
  - Mac build on the Studio runner (`self-hosted, macOS, ARM64`).
  - Windows build on `blacksmith-4vcpu-windows-2025`.
  - The Studio job never runs on `pull_request`.
  - Its `CARGO_TARGET_DIR` and DMG output go on `/Volumes/PRO-G40`. The job stops if the drive is not mounted.
  - No GitHub-hosted labels anywhere.
- Checks that must pass before a release:
  - Engine unit tests.
  - The bash-versus-Rust parity test against a fake provider server.
  - MCP selftest against the built binary.
  - The key-leak scan over files written in a test run.
  - The provider link check.
  - A clean-machine install on a Win11 VM and on a Mac user account with no developer tools.

## 11. Out of scope for v1

- Linux builds. Tauri can add them later from the same code.
- iPhone, iPad and Android.
- A claude.ai web or mobile connector. That needs a hosted remote MCP server and accounts.
- Hosted keys, sign-in, billing, or any Frontier Infra server.
- A single-key mode, such as one OpenRouter key for every AI. A good consumer follow-up.
- Wiring Cursor or Codex CLI from the app. `roundtable install` still does that for the CLI.
- The `codex` head and the Grok CLI and Hermes fallbacks in the UI.
- Any change to the bash engine, `pyproject.toml`, `README.md`, `AGENTS.md`, `.claude-plugin/` or the roundtable.sh site. Another workflow owns those.
- Translations. English only.
- PDFs and images as background files.

## 12. Open questions for Jason

1. **Public launch.** Signed installers and a download button on roundtable.sh are a public launch. The build proceeds now. The release waits for his yes.
2. **Publisher name.** The signatures show the vault accounts' legal name, likely Titanium Computing or Jason. Ship under that name, or get a Frontier Infra signing identity first?
3. **Local history.** Saving past questions on the person's computer stores their content. It is local only, with an off switch. Keep it on by default?
4. **Support address.** Help links to GitHub issues, as the README does. Use help@myagents.email instead, with `/support/llms.txt` and `/support/agent.md` on roundtable.sh?
