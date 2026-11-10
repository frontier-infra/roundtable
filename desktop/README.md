# Roundtable Desktop

The Mac and Windows app for Roundtable. Tauri 2, a Rust core and a plain TypeScript UI.
The design is in [SPEC.md](SPEC.md).

## Layout

| Folder | What it is |
|---|---|
| `engine/` | The `roundtable` engine in Rust: the council, the chair, the MCP server for Claude, keys and history. Ships inside the app as one binary. |
| `core/` | The Tauri app: engine setup, Claude connection, commands for the UI, the updater. |
| `ui/` | The TypeScript UI. No framework. `src/mock.ts` is a fake backend for tests and the browser preview. |
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
Preview query options: `screen=1..5`, `view=main|settings`, `claude=none|app|code|both|plugin`, `keys=claude,gemini`, `os=windows`, `history=1`, `run=setup|quick|talk`, `clis=some|none` (AI apps found on the computer).

`roundtable detect` lists the AI apps on this computer and whether each is signed in. It sends no prompt.
A debug build opens on a given screen with `ROUNDTABLE_START_SCREEN=3` (or `settings`), for screenshots.

## Releases

A `desktop-v0.1.N` tag runs `.github/workflows/desktop-release.yml` on Blacksmith runners only (this repo is public, so no self-hosted runner):

- Mac: universal `.dmg`, signed with the Developer ID, notarized and stapled.
- Windows: per-user NSIS `.exe`, signed with Azure Trusted Signing (`desktop/scripts/sign-windows.ps1`).
- The updater manifest goes to the tag release and to the rolling `desktop-latest` release.

A tag with a dash, such as `desktop-v0.1.1-rc1`, is a test build. It uploads workflow artifacts and publishes nothing.

Release secrets (GitHub repository secrets, values in Bitwarden):

- Updater: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from the item "Roundtable Desktop updater key".
- Mac certificate: `APPLE_CERTIFICATE` (the `P12_BASE64` attachment), `APPLE_CERTIFICATE_PASSWORD` (field `IMPORT_PASSWORD`), `APPLE_TEAM_ID` from "Apple Developer".
- Mac notarization: `APPLE_API_KEY_P8` (the `.p8` attachment, base64), `APPLE_API_KEY_ID` (field `KEY_ID`), `APPLE_API_ISSUER_ID` (in the notes) from "titanium-bot-ci".
- Windows: `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID` from the item whose name starts "Azure Trusted Signing" (Entra app titanium-gh-signing). There is no client secret: GitHub signs in with OIDC.

Windows signing also needs two things outside this repo:

1. A federated credential on the Entra app `titanium-gh-signing`: issuer `https://token.actions.githubusercontent.com`, subject `repo:frontier-infra@276834721/roundtable@1272416852:environment:desktop-release` (the immutable, ID-based form; the repo's OIDC subject setting has `use_immutable_subject` on), audience `api://AzureADTokenExchange`.
2. The repository variable `WINDOWS_SIGNING` set to `on`. Until then the Windows job builds an artifact named `roundtable-desktop-windows-UNSIGNED`, and a release tag fails at publish.
