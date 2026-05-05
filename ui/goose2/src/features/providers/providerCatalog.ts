import type { ProviderCatalogEntry } from "@/shared/types/providers";
import { useProviderCatalogStore } from "./stores/providerCatalogStore";
import { normalizeProviderKey } from "./lib/providerKey";

export { normalizeProviderKey };

export function getProviderCatalog(): ProviderCatalogEntry[] {
  return useProviderCatalogStore.getState().entries;
}

export function getCatalogEntry(
  providerId: string,
): ProviderCatalogEntry | undefined {
  return getCatalogEntryFromEntries(getProviderCatalog(), providerId);
}

export function getAgentProviders(): ProviderCatalogEntry[] {
  return getAgentProvidersFromEntries(getProviderCatalog());
}

export function getModelProviders(): ProviderCatalogEntry[] {
  return getModelProvidersFromEntries(getProviderCatalog());
}

export function getCatalogEntryFromEntries(
  entries: ProviderCatalogEntry[],
  providerId: string,
): ProviderCatalogEntry | undefined {
  return entries.find((provider) => provider.id === providerId);
}

export function getAgentProvidersFromEntries(
  entries: ProviderCatalogEntry[],
): ProviderCatalogEntry[] {
  return entries.filter((provider) => provider.category === "agent");
}

export function getModelProvidersFromEntries(
  entries: ProviderCatalogEntry[],
): ProviderCatalogEntry[] {
  return entries.filter((provider) => provider.category === "model");
}

export function resolveAgentProviderCatalogIdStrictFromEntries(
  entries: ProviderCatalogEntry[],
  providerId: string,
): string | null {
  const directMatch = entries.find((provider) => provider.id === providerId);
  if (directMatch?.category === "agent") {
    return directMatch.id;
  }

  const normalized = normalizeProviderKey(providerId);
  for (const provider of entries) {
    if (provider.category !== "agent") {
      continue;
    }
    const aliases = [provider.id, ...(provider.aliases ?? [])];
    if (aliases.some((alias) => normalizeProviderKey(alias) === normalized)) {
      return provider.id;
    }
  }

  return null;
}

export function resolveAgentProviderCatalogIdStrict(
  providerId: string,
): string | null {
  return resolveAgentProviderCatalogIdStrictFromEntries(
    getProviderCatalog(),
    providerId,
  );
}

function normalizedAliasMatchesCandidate(alias: string, candidate: string) {
  const normalizedAlias = normalizeProviderKey(alias);
  if (!normalizedAlias) {
    return false;
  }

  const candidates = new Set([candidate]);
  if (candidate.endsWith("_acp")) {
    candidates.add(candidate.slice(0, -"_acp".length));
  }

  return candidates.has(normalizedAlias);
}

export function resolveAgentProviderCatalogIdFromEntries(
  entries: ProviderCatalogEntry[],
  providerId: string,
  label?: string,
): string | null {
  const directMatch = resolveAgentProviderCatalogIdStrictFromEntries(
    entries,
    providerId,
  );
  if (directMatch) {
    return directMatch;
  }

  const normalizedCandidates = [providerId, label ?? ""]
    .map((value) => normalizeProviderKey(value))
    .filter(Boolean);

  for (const candidate of normalizedCandidates) {
    for (const provider of entries) {
      if (provider.category !== "agent") {
        continue;
      }
      for (const alias of [provider.id, ...(provider.aliases ?? [])]) {
        if (normalizedAliasMatchesCandidate(alias, candidate)) {
          return provider.id;
        }
      }
    }
  }

  return null;
}

export function resolveAgentProviderCatalogId(
  providerId: string,
  label?: string,
): string | null {
  return resolveAgentProviderCatalogIdFromEntries(
    getProviderCatalog(),
    providerId,
    label,
  );
}
