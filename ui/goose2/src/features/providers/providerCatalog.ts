import type { ProviderCatalogEntry } from "@/shared/types/providers";
import {
  AGENT_PROVIDER_ALIAS_MAP,
  AGENT_PROVIDER_FUZZY_MATCHERS,
  normalizeProviderKey,
} from "./providerCatalogAliases";
import {
  AGENT_PROVIDER_CATALOG,
  MODEL_PROVIDER_CATALOG,
} from "./providerCatalogEntries";

export const PROVIDER_CATALOG: ProviderCatalogEntry[] = [
  ...AGENT_PROVIDER_CATALOG,
  ...MODEL_PROVIDER_CATALOG,
];

export function getCatalogEntry(
  providerId: string,
): ProviderCatalogEntry | undefined {
  return PROVIDER_CATALOG.find((p) => p.id === providerId);
}

export function getAgentProviders(): ProviderCatalogEntry[] {
  return AGENT_PROVIDER_CATALOG;
}

export function getModelProviders(): ProviderCatalogEntry[] {
  return MODEL_PROVIDER_CATALOG;
}

export function resolveAgentProviderCatalogIdStrict(
  providerId: string,
): string | null {
  const directMatch = getAgentProviders().find(
    (provider) => provider.id === providerId,
  );
  if (directMatch) {
    return directMatch.id;
  }

  const normalized = normalizeProviderKey(providerId);
  const aliasMatch = AGENT_PROVIDER_ALIAS_MAP[normalized];
  if (aliasMatch) {
    return aliasMatch;
  }

  return null;
}

export function resolveAgentProviderCatalogId(
  providerId: string,
  label?: string,
): string | null {
  const directMatch = getAgentProviders().find(
    (provider) => provider.id === providerId,
  );
  if (directMatch) {
    return directMatch.id;
  }

  const normalizedCandidates = [providerId, label ?? ""]
    .map((value) => normalizeProviderKey(value))
    .filter(Boolean);

  for (const candidate of normalizedCandidates) {
    const aliasMatch = AGENT_PROVIDER_ALIAS_MAP[candidate];
    if (aliasMatch) {
      return aliasMatch;
    }
  }

  for (const candidate of normalizedCandidates) {
    for (const [needle, catalogId] of AGENT_PROVIDER_FUZZY_MATCHERS) {
      if (candidate.includes(needle)) {
        return catalogId;
      }
    }
  }

  return null;
}
