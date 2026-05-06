import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { resolveAgentProviderCatalogIdStrictFromEntries } from "@/features/providers/providerCatalog";
import type { ProviderCatalogEntry } from "@/shared/types/providers";

interface ResolveSelectedAgentIdOptions {
  catalogEntries: ProviderCatalogEntry[];
  catalogLoaded: boolean;
  selectedProvider?: string;
  getProviderInventoryEntry: (
    providerId: string,
  ) => ProviderInventoryEntryDto | undefined;
}

export function resolveSelectedAgentId({
  catalogEntries,
  catalogLoaded,
  selectedProvider,
  getProviderInventoryEntry,
}: ResolveSelectedAgentIdOptions): string {
  if (!selectedProvider) {
    return "goose";
  }

  const resolvedAgentId = resolveAgentProviderCatalogIdStrictFromEntries(
    catalogEntries,
    selectedProvider,
  );
  if (resolvedAgentId) {
    return resolvedAgentId;
  }

  if (!catalogLoaded) {
    const inventoryEntry = getProviderInventoryEntry(selectedProvider);
    if (inventoryEntry?.category === "agent") {
      return selectedProvider;
    }
  }

  return "goose";
}
