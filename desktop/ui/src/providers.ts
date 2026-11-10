import type { HeadId } from "./api";

export interface Provider {
  id: HeadId;
  name: string;
  company: string;
  keyPage: string;
  /** Shown first on screen 3; the rest sit under "More AIs". */
  primary: boolean;
  defaultModel: string;
  tag?: string;
}

// Key pages and default models match desktop/engine/src/providers.rs.
export const PROVIDERS: Provider[] = [
  {
    id: "claude",
    name: "Claude",
    company: "Anthropic",
    keyPage: "https://console.anthropic.com/settings/keys",
    primary: true,
    defaultModel: "claude-opus-4-8",
    tag: "Recommended. Claude also acts as the chair.",
  },
  {
    id: "gemini",
    name: "Gemini",
    company: "Google",
    keyPage: "https://aistudio.google.com/apikey",
    primary: true,
    defaultModel: "gemini-3.1-pro-preview",
  },
  {
    id: "openai",
    name: "ChatGPT",
    company: "OpenAI",
    keyPage: "https://platform.openai.com/api-keys",
    primary: false,
    defaultModel: "gpt-5.5",
  },
  { id: "grok", name: "Grok", company: "xAI", keyPage: "https://console.x.ai", primary: false, defaultModel: "grok-4.3" },
  {
    id: "glm",
    name: "GLM",
    company: "Z.ai",
    keyPage: "https://z.ai/manage-apikey/apikey-list",
    primary: false,
    defaultModel: "glm-5.2",
  },
  {
    id: "minimax",
    name: "MiniMax",
    company: "MiniMax",
    keyPage: "https://platform.minimax.io/user-center/basic-information/interface-key",
    primary: false,
    defaultModel: "MiniMax-M3",
  },
];

export const CHAIR_DEFAULT_MODEL = "claude-opus-4-8";

/** Heads that only run through a local tool. Ids match desktop/engine/src/providers.rs. */
export const CLI_ONLY_NAMES: Record<string, string> = {
  codex: "Codex",
  qwen: "Qwen",
  kimi: "Kimi",
  opencode: "OpenCode",
  cursor: "Cursor",
};

/** Every head, in the order it sits at the table. */
export const HEAD_ORDER = ["claude", "codex", "gemini", "openai", "grok", "glm", "minimax", "qwen", "kimi", "opencode", "cursor"];

export function providerName(id: string): string {
  return PROVIDERS.find((p) => p.id === id)?.name ?? CLI_ONLY_NAMES[id] ?? id;
}

export const HELP_URL = "https://roundtable.sh";
export const ISSUES_URL = "https://github.com/frontier-infra/roundtable/issues/new";
export const CLAUDE_DOWNLOAD_URL = "https://claude.ai/download";
