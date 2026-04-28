import type {
  ProviderInventoryEntryDto,
  RefreshProviderInventoryResponse,
} from "@aaif/goose-sdk";
import { getProviderInventory, refreshProviderInventory } from "./inventory";

export const INVENTORY_POLL_DELAYS_MS = [250, 500, 750, 1000, 1500, 2000];

type GetProviderInventory = typeof getProviderInventory;
type RefreshProviderInventory = typeof refreshProviderInventory;

interface SyncProviderInventoryOptions {
  getInventory?: GetProviderInventory;
  refreshInventory?: RefreshProviderInventory;
  initialRefresh?: RefreshProviderInventoryResponse;
  onEntries?: (entries: ProviderInventoryEntryDto[]) => void;
  sleep?: (ms: number) => Promise<void>;
}

export interface SyncProviderInventoryResult {
  entries: ProviderInventoryEntryDto[];
  refresh: RefreshProviderInventoryResponse;
  settled: boolean;
  polledProviderIds: string[];
}

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function unique(values: string[]): string[] {
  return [...new Set(values.filter(Boolean))];
}

function mergeEntries(
  current: Map<string, ProviderInventoryEntryDto>,
  entries: ProviderInventoryEntryDto[],
) {
  for (const entry of entries) {
    current.set(entry.providerId, entry);
  }
}

function skippedProviderIds(refresh: RefreshProviderInventoryResponse) {
  return (refresh.skipped ?? []).map((skip) => skip.providerId);
}

function alreadyRefreshingProviderIds(
  refresh: RefreshProviderInventoryResponse,
) {
  return (refresh.skipped ?? [])
    .filter((skip) => skip.reason === "already_refreshing")
    .map((skip) => skip.providerId);
}

export async function syncProviderInventory(
  providerIds: string[],
  {
    getInventory = getProviderInventory,
    refreshInventory = refreshProviderInventory,
    initialRefresh,
    onEntries,
    sleep = defaultSleep,
  }: SyncProviderInventoryOptions = {},
): Promise<SyncProviderInventoryResult> {
  const refresh = initialRefresh ?? (await refreshInventory(providerIds));
  const entriesByProviderId = new Map<string, ProviderInventoryEntryDto>();
  const immediateProviderIds = unique([
    ...providerIds,
    ...refresh.started,
    ...skippedProviderIds(refresh),
  ]);
  const immediateEntries = await getInventory(immediateProviderIds);
  mergeEntries(entriesByProviderId, immediateEntries);
  onEntries?.(immediateEntries);

  const polledProviderIds = unique([
    ...providerIds,
    ...refresh.started,
    ...alreadyRefreshingProviderIds(refresh),
  ]);
  if (
    polledProviderIds.length === 0 ||
    immediateEntries.every((entry) => !entry.refreshing)
  ) {
    return {
      entries: [...entriesByProviderId.values()],
      refresh,
      settled: true,
      polledProviderIds,
    };
  }

  for (const delayMs of INVENTORY_POLL_DELAYS_MS) {
    await sleep(delayMs);
    const entries = await getInventory(polledProviderIds);
    mergeEntries(entriesByProviderId, entries);
    onEntries?.(entries);
    if (entries.every((entry) => !entry.refreshing)) {
      return {
        entries: [...entriesByProviderId.values()],
        refresh,
        settled: true,
        polledProviderIds,
      };
    }
  }

  return {
    entries: [...entriesByProviderId.values()],
    refresh,
    settled: false,
    polledProviderIds,
  };
}
