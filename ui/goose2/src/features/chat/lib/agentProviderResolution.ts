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
    // Catalog not loaded and no inventory info — preserve the stored
    // selection so the UI doesn't briefly flash "Goose" before validation
    // completes.  Fall back to "goose" only when there is no selection or
    // after the catalog has loaded and proven the provider is not an agent.
    if (!inventoryEntry) {
      return selectedProvider;
    }
  }

  return "goose";
}
