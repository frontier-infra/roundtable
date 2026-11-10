# Roundtable Desktop

The Mac and Windows app for Roundtable. Tauri 2, a Rust core and a plain TypeScript UI.
The design is in [SPEC.md](SPEC.md).

## Layout

| Folder | What it is |
|---|---|
| `engine/` | The `roundtable` engine in Rust: the council, the chair, the MCP server for Claude, keys and history. Ships inside the app as one binary. |
| `core/` | The Tauri app: engine setup, Claude connection, commands for the UI, the updater. |
| `ui/` | The TypeScript UI. No framework. `src/mock.ts` is a fake backend for tests and the browser preview. |
| `../docs/app/` | The Help pages, one Markdown file each. The app bundles them; roundtable.sh can import them (see that folder's README). The menu bar is `core/src/menu.rs`. |
| `scripts/` | Sidecar build, bash parity test, link check, updater manifest. |

## Why a Rust engine instead of a Python venv

The task allowed a private Python venv made with uv, or a bundled engine. The app bundles the Rust engine:

- The CLI engine is bash (`lib/core.sh`). Stock Windows has no bash, so a Python venv alone cannot run it.
- A Rust binary needs nothing installed first, on either OS, and is signed with the app.
- One signed binary owns the keychain items, so Claude starting it never raises a macOS password prompt.
- `scripts/parity.py` runs both engines against one fake provider and compares every section.

## Build and test

```sh
# engine and app tests (build the sidecar first, the app crate needs it)
node scripts/sidecar.mjs
cargo test --workspace
python3 scripts/parity.py target/debug/roundtable-engine

# UI
cd ui && npm ci && npx tsc --noEmit && npx vitest run && cd ..
node scripts/check-links.mjs

# the app: .app and .dmg on a Mac, the NSIS .exe on Windows
cd core && cargo tauri build
```

`cargo tauri dev` runs the app against the Vite dev server. Opening `http://localhost:5173/` in a plain browser shows the UI on the fake backend.
Preview query options: `screen=1..5`, `view=main|settings|help` (`page=<id>`, `q=<search>`), `about=1`, `claude=none|app|code|both|plugin`, `keys=claude,gemini`, `os=windows`, `history=1`, `run=setup|quick|talk`, `clis=some|none` (AI apps found on the computer), `providers=1` (Ollama found here, OpenRouter added).

### Other providers

Settings, AI accounts, "Add a provider": OpenRouter, Ollama, LM Studio, Groq, Together, or Custom (any OpenAI-compatible base URL).
Every ticked model is its own seat, `provider:model` (`ollama:qwen3:4b`, `openrouter:openai/gpt-4o-mini`), labelled with the model and the provider.
A seat is asked with one plain chat completion: one user message, no tools, no system prompt (the rc8 seat isolation rule).
Keys go to the OS keychain under `provider-<id>`, or come from the environment (`OPENROUTER_API_KEY`, `GROQ_API_KEY`, `TOGETHER_API_KEY`, `ROUNDTABLE_PROVIDER_<ID>_KEY`).
Ollama and LM Studio are found running the way AI apps are, and say "free, on this computer".
The model list is `GET /v1/models` (Ollama also `/api/tags`); OpenRouter's per-model prices feed the cost line under the ask box, else it says unknown.
A model named Jebadiah counts as ours only when it is a frontier-infra build (the rule from fix/detect-ours).
With no way to ask Claude, the first ticked provider seat chairs, so the coach, the suggested panel and the verdict still work.
The bash CLI (`lib/core.sh`) keeps its current seats; provider seats are in the Rust engine and the app.

```sh
roundtable provider detect
echo "$KEY" | roundtable provider add openrouter --key-stdin --check --models openai/gpt-4o-mini
roundtable provider add custom --name "My proxy" --url https://host/v1 --models small-a,small-b
roundtable provider models openrouter --search "llama 70b"
roundtable provider list
roundtable ask -q "..." --seats ollama:qwen3:4b,openrouter:openai/gpt-4o-mini --rounds 2
roundtable provider remove openrouter
```

The MCP tool takes the same seats as `seats` (a comma list or an array).

`roundtable detect` lists the AI apps on this computer and whether each is signed in. It sends no prompt.
`roundtable test <ai> [--via auto|app|key]` asks one AI "Answer in five words: is water wet?" and prints the command, exit code, time and answer.

Release candidates share one version number, so the app re-copies its engine whenever the installed copy's bytes differ from the one inside the app (rc4 had kept running an rc1 engine with no app route).
A debug build opens on a given screen with `ROUNDTABLE_START_SCREEN=3` (or `settings`), for screenshots.

## Releases

### Versions

One number for the app, the engine and the UI: `desktop/Cargo.toml`, `[workspace.package]`. Tauri reads it from there, and the UI asks the core. `desktop/scripts/version.mjs` reads and writes it.

- The repo holds the next release, such as `0.1.2`. After a release, `node desktop/scripts/version.mjs bump` moves it on.
- The patch digit climbs to 99 (0.1.8, 0.1.9 ... 0.1.99), then 0.2.0. Only Jason moves the minor digit early.
- A test build is `0.1.2-rc.1`, `0.1.2-rc.2` and so on, with a dot. The release workflow stamps that number into the build, so every test build reports its own version. Semver puts `0.1.2-rc.10` after `rc.9` and every rc before `0.1.2`.
- The About section and Settings, Updates show the version.

### Changelog

`desktop/CHANGELOG.md`, in plain words, with Added, Improved and Fixed under `## X.Y.Z - date`. Every release needs its section: the tag fails without one. The release workflow puts it in the GitHub release notes and the update manifest. The app ships the file for What's new, and roundtable.sh/changelog reads it.

### Release tags

A tag runs `.github/workflows/desktop-release.yml` on Blacksmith runners only (this repo is public, so no self-hosted runner):

- Mac: universal `.dmg`, signed with the Developer ID, notarized and stapled, and the updater bundle `Roundtable_<version>_universal.app.tar.gz` with its `.sig`.
- Windows: per-user NSIS `.exe`, signed with Azure Trusted Signing (`desktop/scripts/sign-windows.ps1`), with its updater `.sig`.

| Tag | Channel | Where it goes |
|---|---|---|
| `desktop-v0.1.2-rc.1` | Test builds | The rolling pre-release `desktop-test` and its `latest.json`. Only apps with Settings, Updates, **Test builds** on read it. It keeps the newest two test builds. |
| `desktop-v0.1.2` | Stable | The release `desktop-v0.1.2`, the rolling `desktop-latest` manifest every app reads, and `desktop-test` too. Jason approves every stable tag. |

### Updates in the app

- Checks at launch and every 6 hours: `desktop-latest/latest.json`, or `desktop-test/latest.json` with Test builds on.
- A newer version shows a banner: "Roundtable 0.1.3 is ready." with **What's new**, **Update now** and **Later**.
- "Install updates automatically" (on by default) downloads in the background and installs on the next quit. **Update now** installs and restarts at once.
- The first launch after an update opens What's new for every version since the last one seen. Settings, Updates has **Check now** and **What's new**.
- Every package is checked against the updater public key in `core/tauri.conf.json` before it installs.

Release secrets (GitHub repository secrets, values in Bitwarden):

- Updater: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from the item "Roundtable Desktop updater key".
- Mac certificate: `APPLE_CERTIFICATE` (the `P12_BASE64` attachment), `APPLE_CERTIFICATE_PASSWORD` (field `IMPORT_PASSWORD`), `APPLE_TEAM_ID` from "Apple Developer".
- Mac notarization: `APPLE_API_KEY_P8` (the `.p8` attachment, base64), `APPLE_API_KEY_ID` (field `KEY_ID`), `APPLE_API_ISSUER_ID` (in the notes) from "titanium-bot-ci".
- Windows: `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID` from the item whose name starts "Azure Trusted Signing" (Entra app titanium-gh-signing). There is no client secret: GitHub signs in with OIDC.

Windows signing also needs two things outside this repo:

1. A federated credential on the Entra app `titanium-gh-signing`: issuer `https://token.actions.githubusercontent.com`, subject `repo:frontier-infra@276834721/roundtable@1272416852:environment:desktop-release` (the immutable, ID-based form; the repo's OIDC subject setting has `use_immutable_subject` on), audience `api://AzureADTokenExchange`.
2. The repository variable `WINDOWS_SIGNING` set to `on`. Until then the Windows job builds an artifact named `roundtable-desktop-windows-UNSIGNED`, and a release tag fails at publish.
