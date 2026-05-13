import type {
  ProviderInventoryEntryDto,
  RefreshProviderInventoryResponse,
} from "@aaif/goose-sdk";
import { getClient } from "@/shared/api/acpConnection";
import { perfLog } from "@/shared/lib/perfLog";

export async function getProviderInventory(
  providerIds: string[] = [],
): Promise<ProviderInventoryEntryDto[]> {
  const client = await getClient();
  const t0 = performance.now();
  const response = await client.goose.GooseProvidersList({ providerIds });
  perfLog(
    `[perf:inventory] getProviderInventory done in ${(performance.now() - t0).toFixed(1)}ms (n=${response.entries.length})`,
  );
  return response.entries;
}

export async function refreshProviderInventory(
  providerIds: string[] = [],
): Promise<RefreshProviderInventoryResponse> {
  const client = await getClient();
  const t0 = performance.now();
  const response = await client.goose.GooseProvidersInventoryRefresh({
    providerIds,
  });
  perfLog(
    `[perf:inventory] refreshProviderInventory done in ${(performance.now() - t0).toFixed(1)}ms started=[${response.started.join(",")}]`,
  );
  return response;
}

/**
 * Refresh configured provider inventories in the background, polling until
 * all providers finish refreshing. If no entries are supplied, fetch and merge
 * the current inventory snapshot first so the UI sees fresh cached data even
 * when no refresh starts.
 *
 * Does NOT set the store's `loading` flag, so the UI keeps showing cached data
 * during the refresh.
 */
export async function backgroundRefreshInventory(
  inventoryStore: {
    mergeEntries: (entries: ProviderInventoryEntryDto[]) => void;
  },
  initialEntries?: ProviderInventoryEntryDto[],
): Promise<void> {
  const entries = initialEntries?.length
    ? initialEntries
    : await getProviderInventory();

  if (!initialEntries?.length) {
    inventoryStore.mergeEntries(entries);
  }

  const configuredProviderIds = entries
    .filter((entry) => entry.configured)
    .map((entry) => entry.providerId);
  if (configuredProviderIds.length === 0) return;

  const refresh = await refreshProviderInventory(configuredProviderIds);
  if (refresh.started.length === 0 && (refresh.skipped ?? []).length === 0) {
    return;
  }

  const { syncProviderInventory } = await import("./inventorySync");
  await syncProviderInventory(configuredProviderIds, {
    initialRefresh: refresh,
    onEntries: (entries) => inventoryStore.mergeEntries(entries),
  });
}
