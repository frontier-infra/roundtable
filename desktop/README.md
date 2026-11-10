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
Preview query options: `screen=1..5`, `view=main|settings`, `claude=none|app|code|both|plugin`, `keys=claude,gemini`, `os=windows`, `history=1`, `run=setup|quick|talk`.

## Releases

A `desktop-v0.1.N` tag runs `.github/workflows/desktop-release.yml`:

- Mac: universal `.dmg`, signed and notarized, built on the Studio runner with output on `/Volumes/PRO-G40`.
- Windows: per-user NSIS `.exe`, signed with Azure Trusted Signing, built on Blacksmith.
- The updater manifest goes to the tag release and to the rolling `desktop-latest` release.

Release secrets (GitHub repository secrets, values in Bitwarden):

- Updater: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from the item "Roundtable Desktop updater key".
- Mac: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` from "Apple Developer".
- Windows: `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`, `TRUSTED_SIGNING_ENDPOINT`, `TRUSTED_SIGNING_ACCOUNT`, `TRUSTED_SIGNING_PROFILE` from the Trusted Signing item.
