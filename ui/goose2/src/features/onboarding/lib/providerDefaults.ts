import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";

export const PROMOTED_MODEL_ORDER = [
  "chatgpt_codex",
  "anthropic",
  "openai",
  "google",
  "ollama",
  "openrouter",
];

export function firstUsableModel(entry: ProviderInventoryEntryDto) {
  return (
    entry.models.find((model) => model.recommended) ??
    entry.models.find((model) => model.id === entry.defaultModel) ??
    entry.models[0]
  );
}
