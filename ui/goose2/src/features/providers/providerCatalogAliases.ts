export function normalizeProviderKey(value: string): string {
  return value
    .toLowerCase()
    .split(/[-_\s]+/)
    .filter(Boolean)
    .join("_");
}

export const AGENT_PROVIDER_ALIAS_MAP: Record<string, string> = {
  goose: "goose",
  claude_acp: "claude-acp",
  claude_code: "claude-acp",
  claude: "claude-acp",
  codex_acp: "codex-acp",
  codex_cli: "codex-acp",
  codex: "codex-acp",
  copilot_acp: "copilot-acp",
  github_copilot: "copilot-acp",
  github_copilot_cli: "copilot-acp",
  cursor_agent: "cursor-agent",
  cursor: "cursor-agent",
  amp_acp: "amp-acp",
  amp: "amp-acp",
  pi_acp: "pi-acp",
  pi: "pi-acp",
};

export const AGENT_PROVIDER_FUZZY_MATCHERS: Array<[string, string]> = [
  ["goose", "goose"],
  ["claude", "claude-acp"],
  ["codex", "codex-acp"],
  ["cursor", "cursor-agent"],
  ["copilot", "copilot-acp"],
  ["amp", "amp-acp"],
];
