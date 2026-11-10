---
description: Check which Roundtable heads are ready and how to add API keys
---

Run `roundtable doctor` in the Bash tool and show the user which heads are configured.

Then tell the user how to add keys, in plain words:

1. In Claude Code, run `/plugin`, pick Roundtable, and choose Configure. Paste any API keys
   you have. Every key is optional. Any one key is enough to start.
2. Or, in a terminal, run `roundtable auth`. It asks for each key and hides what you type.

Never ask the user to paste a key into this chat.
