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
  const seat = splitSeat(id);
  if (seat) return `${seat.model} (${addedNames[seat.provider] ?? PRESET_NAMES[seat.provider] ?? seat.provider})`;
  return PROVIDERS.find((p) => p.id === id)?.name ?? CLI_ONLY_NAMES[id] ?? PRESET_NAMES[id] ?? id;
}

// ---- Seats from providers the person adds ----

/** Preset names, as desktop/engine/src/custom/mod.rs lists them. */
export const PRESET_NAMES: Record<string, string> = {
  openrouter: "OpenRouter",
  ollama: "Ollama",
  lmstudio: "LM Studio",
  groq: "Groq",
  together: "Together",
  custom: "Custom",
};

/** The order "Add a provider" offers them. */
export const PRESET_ORDER = ["openrouter", "ollama", "lmstudio", "groq", "together", "custom"];

export const LOCAL_NOTE = "free, on this computer";

/** Names of the added providers, by id, for seat labels. */
const addedNames: Record<string, string> = {};

/** Remember the added providers' names (after each provider list). */
export function setProviderNames(list: { id: string; name: string }[]): void {
  for (const k of Object.keys(addedNames)) delete addedNames[k];
  for (const p of list) addedNames[p.id] = p.name;
}

/** A provider seat id is `<provider>:<model>`; the provider part ends at the first colon. */
export function splitSeat(id: string): { provider: string; model: string } | null {
  const i = id.indexOf(":");
  if (i <= 0 || i === id.length - 1) return null;
  const provider = id.slice(0, i);
  if (provider.includes("/") || provider.includes("@")) return null;
  return { provider, model: id.slice(i + 1) };
}

export function isProviderSeat(id: string): boolean {
  return splitSeat(id) !== null;
}

/** The family a head belongs to, for mixing providers: the provider on a provider seat, else the head. */
export function family(head: string): string {
  return splitSeat(head)?.provider ?? head;
}

export const HELP_URL = "https://roundtable.sh";
export const ISSUES_URL = "https://github.com/frontier-infra/roundtable/issues/new";
export const CLAUDE_DOWNLOAD_URL = "https://claude.ai/download";
