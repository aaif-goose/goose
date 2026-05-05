import type { ProviderCatalogEntry } from "@/shared/types/providers";
import { useProviderCatalogStore } from "./stores/providerCatalogStore";

export function normalizeProviderKey(value: string): string {
  return value
    .toLowerCase()
    .split(/[-_\s]+/)
    .filter(Boolean)
    .join("_");
}

export function getProviderCatalog(): ProviderCatalogEntry[] {
  return useProviderCatalogStore.getState().entries;
}

export function getCatalogEntry(
  providerId: string,
): ProviderCatalogEntry | undefined {
  return useProviderCatalogStore.getState().entriesById.get(providerId);
}

export function getAgentProviders(): ProviderCatalogEntry[] {
  return getProviderCatalog().filter(
    (provider) => provider.category === "agent",
  );
}

export function getModelProviders(): ProviderCatalogEntry[] {
  return getProviderCatalog().filter(
    (provider) => provider.category === "model",
  );
}

export function resolveAgentProviderCatalogIdStrict(
  providerId: string,
): string | null {
  if (providerId === "goose") {
    return "goose";
  }

  const directMatch = getAgentProviders().find(
    (provider) => provider.id === providerId,
  );
  if (directMatch) {
    return directMatch.id;
  }

  const normalized = normalizeProviderKey(providerId);
  return (
    useProviderCatalogStore.getState().agentAliasMap.get(normalized) ?? null
  );
}

export function resolveAgentProviderCatalogId(
  providerId: string,
  label?: string,
): string | null {
  const directMatch = resolveAgentProviderCatalogIdStrict(providerId);
  if (directMatch) {
    return directMatch;
  }

  const aliasMap = useProviderCatalogStore.getState().agentAliasMap;
  const normalizedCandidates = [providerId, label ?? ""]
    .map((value) => normalizeProviderKey(value))
    .filter(Boolean);

  for (const candidate of normalizedCandidates) {
    const aliasMatch = aliasMap.get(candidate);
    if (aliasMatch) {
      return aliasMatch;
    }
  }

  return null;
}
