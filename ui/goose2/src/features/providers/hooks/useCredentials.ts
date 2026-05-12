import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import {
  getProviderConfig,
  saveProviderConfig,
  deleteProviderConfig,
  type ProviderStatus,
  checkAllProviderStatus,
} from "@/features/providers/api/credentials";
import type { ProviderConfigChangeResponse } from "@aaif/goose-sdk";
import { notifyVoiceDictationConfigChanged } from "@/features/chat/lib/voiceInput";
import {
  syncProviderInventory,
  type SyncProviderInventoryResult,
} from "@/features/providers/api/inventorySync";
import { refreshProviderInventory } from "@/features/providers/api/inventory";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import type { ProviderFieldValue } from "@/shared/types/providers";

export interface ProviderFieldSave {
  key: string;
  value: string;
  isSecret: boolean;
}

interface UseCredentialsReturn {
  configuredIds: Set<string>;
  loading: boolean;
  saving: boolean;
  savingProviderIds: Set<string>;
  syncingProviderIds: Set<string>;
  inventoryWarnings: Map<string, string>;
  getConfig: (providerId: string) => Promise<ProviderFieldValue[]>;
  save: (providerId: string, fields: ProviderFieldSave[]) => Promise<void>;
  remove: (providerId: string) => Promise<void>;
  completeNativeSetup: (
    providerId: string,
    result?: ProviderConfigChangeResponse,
  ) => Promise<void>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function inventoryWarning(
  providerId: string,
  result: SyncProviderInventoryResult,
): string | null {
  const entry = result.entries.find((item) => item.providerId === providerId);
  const skipped = result.refresh.skipped?.find(
    (item) => item.providerId === providerId,
  );
  if (skipped?.reason === "not_configured") {
    return null;
  }
  if (skipped?.reason === "unknown_provider") {
    return "Provider inventory is unavailable.";
  }

  if (entry?.lastRefreshError) {
    return entry.lastRefreshError;
  }

  if (!result.settled && entry?.refreshing) {
    return "Model inventory is still refreshing.";
  }

  return null;
}

export function useCredentials(): UseCredentialsReturn {
  const [statuses, setStatuses] = useState<ProviderStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [savingProviderIds, setSavingProviderIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [syncingProviderIds, setSyncingProviderIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [inventoryWarnings, setInventoryWarnings] = useState<
    Map<string, string>
  >(() => new Map());
  const syncRunIds = useRef(new Map<string, number>());

  const refreshStatuses = useCallback(async () => {
    const nextStatuses = await checkAllProviderStatus();
    setStatuses(nextStatuses);
    return nextStatuses;
  }, []);

  const updateProviderStatus = useCallback((status: ProviderStatus) => {
    setStatuses((current) => {
      const next = current.filter(
        (item) => item.providerId !== status.providerId,
      );
      next.push(status);
      return next;
    });
  }, []);

  useEffect(() => {
    refreshStatuses()
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [refreshStatuses]);

  const configuredIds = useMemo(
    () =>
      new Set(statuses.filter((s) => s.isConfigured).map((s) => s.providerId)),
    [statuses],
  );
  const saving = savingProviderIds.size > 0;

  const getConfig = useCallback(async (providerId: string) => {
    return getProviderConfig(providerId);
  }, []);

  const setProviderSaving = useCallback(
    (providerId: string, isSaving: boolean) => {
      setSavingProviderIds((current) => {
        const next = new Set(current);
        if (isSaving) {
          next.add(providerId);
        } else {
          next.delete(providerId);
        }
        return next;
      });
    },
    [],
  );

  const setProviderSyncing = useCallback(
    (providerId: string, isSyncing: boolean) => {
      setSyncingProviderIds((current) => {
        const next = new Set(current);
        if (isSyncing) {
          next.add(providerId);
        } else {
          next.delete(providerId);
        }
        return next;
      });
    },
    [],
  );

  const setProviderInventoryWarning = useCallback(
    (providerId: string, warning: string | null) => {
      setInventoryWarnings((current) => {
        const next = new Map(current);
        if (warning) {
          next.set(providerId, warning);
        } else {
          next.delete(providerId);
        }
        return next;
      });
    },
    [],
  );

  const startInventorySync = useCallback(
    (
      providerId: string,
      initialRefresh?: SyncProviderInventoryResult["refresh"],
    ) => {
      const runId = (syncRunIds.current.get(providerId) ?? 0) + 1;
      syncRunIds.current.set(providerId, runId);
      setProviderSyncing(providerId, true);
      setProviderInventoryWarning(providerId, null);

      void syncProviderInventory([providerId], {
        initialRefresh,
        onEntries: (entries) => {
          if (syncRunIds.current.get(providerId) !== runId) {
            return;
          }
          useProviderInventoryStore.getState().mergeEntries(entries);
        },
      })
        .then((result) => {
          if (syncRunIds.current.get(providerId) !== runId) {
            return;
          }
          setProviderInventoryWarning(
            providerId,
            inventoryWarning(providerId, result),
          );
        })
        .catch((error) => {
          if (syncRunIds.current.get(providerId) !== runId) {
            return;
          }
          setProviderInventoryWarning(providerId, errorMessage(error));
        })
        .finally(() => {
          if (syncRunIds.current.get(providerId) !== runId) {
            return;
          }
          setProviderSyncing(providerId, false);
        });
    },
    [setProviderInventoryWarning, setProviderSyncing],
  );

  const save = useCallback(
    async (providerId: string, fields: ProviderFieldSave[]) => {
      setProviderSaving(providerId, true);
      try {
        const result = await saveProviderConfig(
          providerId,
          fields.map(({ key, value }) => ({ key, value })),
        );
        updateProviderStatus(result.status);
        notifyVoiceDictationConfigChanged();
        startInventorySync(providerId, result.refresh);
      } finally {
        setProviderSaving(providerId, false);
      }
    },
    [setProviderSaving, startInventorySync, updateProviderStatus],
  );

  const remove = useCallback(
    async (providerId: string) => {
      setProviderSaving(providerId, true);
      try {
        const result = await deleteProviderConfig(providerId);
        updateProviderStatus(result.status);
        notifyVoiceDictationConfigChanged();
        startInventorySync(providerId, result.refresh);
      } finally {
        setProviderSaving(providerId, false);
      }
    },
    [setProviderSaving, startInventorySync, updateProviderStatus],
  );

  const completeNativeSetup = useCallback(
    async (providerId: string, result?: ProviderConfigChangeResponse) => {
      if (result) {
        updateProviderStatus(result.status);
        notifyVoiceDictationConfigChanged();
        startInventorySync(providerId, result.refresh);
        return;
      }

      // Native OAuth returns only after the subprocess writes credentials.
      // Inventory refresh invalidates ACP's secret cache before status reads it.
      let initialRefresh: SyncProviderInventoryResult["refresh"] | undefined;
      try {
        initialRefresh = await refreshProviderInventory([providerId]);
      } catch (error) {
        setProviderInventoryWarning(providerId, errorMessage(error));
      }
      await refreshStatuses();
      notifyVoiceDictationConfigChanged();
      startInventorySync(providerId, initialRefresh);
    },
    [
      refreshStatuses,
      setProviderInventoryWarning,
      startInventorySync,
      updateProviderStatus,
    ],
  );

  return {
    configuredIds,
    loading,
    saving,
    savingProviderIds,
    syncingProviderIds,
    inventoryWarnings,
    getConfig,
    save,
    remove,
    completeNativeSetup,
  };
}
