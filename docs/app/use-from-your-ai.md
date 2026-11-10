---
title: Use Roundtable from your AI
description: Ask the roundtable from inside Claude, ChatGPT, Codex, Cursor and other AI apps.
order: 7
online: https://roundtable.sh/docs/mcp-and-harness/
---

Connect Roundtable to your AI, and you can say "ask the roundtable" in a chat. Your AI sends the question to the table and reads back the verdict. The full transcript comes with it.

![Claude app and Claude Code connected](img/claude-connected.png)

## Connect

Settings, Use Roundtable from your AI (Claude is also on setup step 4):

- Claude app found: click Connect to the Claude app.
- Claude Code found: click Connect to Claude Code.
- Codex found: click Connect to Codex. Start a new Codex session.
- Cursor found: click Connect to Cursor. In Cursor, open Settings, then MCP, and turn roundtable on.
- Another AI app? Open Another AI app? Copy its setup, copy the setup and paste it into that app.
- Not on this computer yet? Click Download Claude.
- Using Claude Code on another computer? Paste the two lines shown into Claude Code.

Roundtable adds itself as a tool your AI can call. Your keys stay in your keychain. Your AI never sees them.

## ChatGPT

ChatGPT runs on the web, so it cannot reach this computer by itself. Share with ChatGPT gives Roundtable a private web link.

1. Install cloudflared once. Mac: get it from Cloudflare's downloads page, or with Homebrew. Windows: run winget install --id Cloudflare.cloudflared. The switch says what to do if it is missing.
2. Switch on Share with ChatGPT and click Copy link.
3. In ChatGPT on the web: Settings, Security and login, turn on Developer mode.
4. Open Plugins, click +, paste the link as the MCP server URL, and choose No authentication.
5. In a chat, choose Developer mode from the + menu and pick Roundtable.

The link holds a secret key. Share it with no one. Switch it off and the link stops working. Next time you get a new link. Every call shows under Calls, with the question and the result.

The same switch works for the OpenAI API and Claude connectors. Open For the OpenAI or Claude API for the server address and the token.

## Use it

In your AI, ask for a roundtable in plain words:

- "Ask the roundtable whether we should move the launch to March."
- "Get a second opinion from the roundtable on this plan, two rounds."

The roundtable shows up in your past roundtables in the app, marked Claude, MCP or Remote.

## Disconnect

Settings, Use Roundtable from your AI. Disconnect removes the tool from Claude. Each other app has its own Disconnect. If the Roundtable plugin is also installed in Claude Code, click Use the app's engine instead, so Claude does not see two Roundtable tools.
