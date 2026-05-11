import { useCallback, useMemo, useRef, useState } from "react";
import type { ProviderConfigStatusDto } from "@aaif/goose-sdk";
import {
  createCustomProvider,
  deleteCustomProvider,
  getCustomProviderTemplate,
  listCustomProviderCatalog,
  readCustomProvider,
  updateCustomProvider,
} from "../api/customProviders";
import {
  syncProviderInventory,
  type SyncProviderInventoryResult,
} from "../api/inventorySync";
import {
  assertValidCustomProviderDraft,
  type CustomProviderValidationOptions,
} from "../lib/customProviderValidation";
import { customProviderDraftToUpsertRequest } from "../lib/customProviderDraft";
import type {
  CustomProviderCreateResponse,
  CustomProviderDeleteResponse,
  CustomProviderDraft,
  CustomProviderFormat,
  CustomProviderReadResponse,
  CustomProviderUpdateResponse,
  CustomProviderUpsertRequest,
  ProviderTemplateCatalogEntryDto,
  ProviderTemplateDto,
} from "../lib/customProviderTypes";
import { useProviderInventoryStore } from "../stores/providerInventoryStore";

interface SaveDraftOptions extends CustomProviderValidationOptions {
  providerId?: string;
}

interface UseCustomProvidersReturn {
  catalog: ProviderTemplateCatalogEntryDto[];
  catalogLoading: boolean;
  saving: boolean;
  savingProviderIds: Set<string>;
  deletingProviderIds: Set<string>;
  syncingProviderIds: Set<string>;
  inventoryWarnings: Map<string, string>;
  statusByProviderId: Map<string, ProviderConfigStatusDto>;
  configuredIds: Set<string>;
  loadCatalog: (
    format?: CustomProviderFormat,
  ) => Promise<ProviderTemplateCatalogEntryDto[]>;
  getTemplate: (providerId: string) => Promise<ProviderTemplateDto>;
  read: (providerId: string) => Promise<CustomProviderReadResponse>;
  create: (
    input: CustomProviderUpsertRequest,
  ) => Promise<CustomProviderCreateResponse>;
  update: (
    providerId: string,
    input: CustomProviderUpsertRequest,
  ) => Promise<CustomProviderUpdateResponse>;
  remove: (providerId: string) => Promise<CustomProviderDeleteResponse>;
  saveDraft: (
    draft: CustomProviderDraft,
    options?: SaveDraftOptions,
  ) => Promise<CustomProviderCreateResponse | CustomProviderUpdateResponse>;
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

function useSetMembershipState() {
  const [state, setState] = useState<Set<string>>(() => new Set());

  const setMembership = useCallback((providerId: string, present: boolean) => {
    setState((current) => {
      const next = new Set(current);
      if (present) {
        next.add(providerId);
      } else {
        next.delete(providerId);
      }
      return next;
    });
  }, []);

  return [state, setMembership] as const;
}

export function useCustomProviders(): UseCustomProvidersReturn {
  const catalogRequestIdRef = useRef(0);
  const operationIdRef = useRef(0);
  const deletedProviderIdsRef = useRef(new Set<string>());
  const [catalog, setCatalog] = useState<ProviderTemplateCatalogEntryDto[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [savingProviderIds, setProviderSaving] = useSetMembershipState();
  const [deletingProviderIds, setProviderDeleting] = useSetMembershipState();
  const [syncingProviderIds, setProviderSyncing] = useSetMembershipState();
  const [statusByProviderId, setStatusByProviderId] = useState<
    Map<string, ProviderConfigStatusDto>
  >(() => new Map());
  const [inventoryWarnings, setInventoryWarnings] = useState<
    Map<string, string>
  >(() => new Map());

  const saving = savingProviderIds.size > 0 || deletingProviderIds.size > 0;

  const configuredIds = useMemo(
    () =>
      new Set(
        [...statusByProviderId.values()]
          .filter((status) => status.isConfigured)
          .map((status) => status.providerId),
      ),
    [statusByProviderId],
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

  const updateStatus = useCallback((status: ProviderConfigStatusDto) => {
    setStatusByProviderId((current) => {
      const next = new Map(current);
      next.set(status.providerId, status);
      return next;
    });
  }, []);

  const removeInventoryEntry = useCallback((providerId: string) => {
    const store = useProviderInventoryStore.getState();
    store.setEntries(
      [...store.entries.values()].filter(
        (entry) => entry.providerId !== providerId,
      ),
    );
  }, []);

  const startInventorySync = useCallback(
    (providerId: string, result: SyncProviderInventoryResult["refresh"]) => {
      setProviderSyncing(providerId, true);
      setProviderInventoryWarning(providerId, null);

      void syncProviderInventory([providerId], {
        initialRefresh: result,
        onEntries: (entries) => {
          const visibleEntries = entries.filter(
            (entry) => !deletedProviderIdsRef.current.has(entry.providerId),
          );
          if (visibleEntries.length > 0) {
            useProviderInventoryStore.getState().mergeEntries(visibleEntries);
          }
        },
      })
        .then((syncResult) => {
          setProviderInventoryWarning(
            providerId,
            inventoryWarning(providerId, syncResult),
          );
        })
        .catch((error) => {
          setProviderInventoryWarning(providerId, errorMessage(error));
        })
        .finally(() => setProviderSyncing(providerId, false));
    },
    [setProviderInventoryWarning, setProviderSyncing],
  );

  const loadCatalog = useCallback(async (format?: CustomProviderFormat) => {
    const requestId = catalogRequestIdRef.current + 1;
    catalogRequestIdRef.current = requestId;
    setCatalogLoading(true);
    try {
      const nextCatalog = await listCustomProviderCatalog(format);
      if (catalogRequestIdRef.current === requestId) {
        setCatalog(nextCatalog);
      }
      return nextCatalog;
    } finally {
      if (catalogRequestIdRef.current === requestId) {
        setCatalogLoading(false);
      }
    }
  }, []);

  const read = useCallback(
    async (providerId: string) => {
      const result = await readCustomProvider(providerId);
      updateStatus(result.status);
      return result;
    },
    [updateStatus],
  );

  const create = useCallback(
    async (input: CustomProviderUpsertRequest) => {
      const pendingId = `create-${operationIdRef.current + 1}`;
      operationIdRef.current += 1;
      setProviderSaving(pendingId, true);
      try {
        const result = await createCustomProvider(input);
        deletedProviderIdsRef.current.delete(result.providerId);
        updateStatus(result.status);
        startInventorySync(result.providerId, result.refresh);
        return result;
      } finally {
        setProviderSaving(pendingId, false);
      }
    },
    [setProviderSaving, startInventorySync, updateStatus],
  );

  const update = useCallback(
    async (providerId: string, input: CustomProviderUpsertRequest) => {
      setProviderSaving(providerId, true);
      try {
        const result = await updateCustomProvider(providerId, input);
        deletedProviderIdsRef.current.delete(result.providerId);
        updateStatus(result.status);
        startInventorySync(result.providerId, result.refresh);
        return result;
      } finally {
        setProviderSaving(providerId, false);
      }
    },
    [setProviderSaving, startInventorySync, updateStatus],
  );

  const remove = useCallback(
    async (providerId: string) => {
      setProviderDeleting(providerId, true);
      deletedProviderIdsRef.current.add(providerId);
      try {
        const result = await deleteCustomProvider(providerId);
        setStatusByProviderId((current) => {
          const next = new Map(current);
          next.set(providerId, { providerId, isConfigured: false });
          return next;
        });
        removeInventoryEntry(providerId);
        return result;
      } catch (error) {
        deletedProviderIdsRef.current.delete(providerId);
        throw error;
      } finally {
        setProviderDeleting(providerId, false);
      }
    },
    [removeInventoryEntry, setProviderDeleting],
  );

  const saveDraft = useCallback(
    async (draft: CustomProviderDraft, options: SaveDraftOptions = {}) => {
      assertValidCustomProviderDraft(draft, options);
      const providerId = options.providerId ?? draft.providerId;
      const input = customProviderDraftToUpsertRequest(draft);

      if (providerId) {
        return update(providerId, input);
      }

      return create(input);
    },
    [create, update],
  );

  return {
    catalog,
    catalogLoading,
    saving,
    savingProviderIds,
    deletingProviderIds,
    syncingProviderIds,
    inventoryWarnings,
    statusByProviderId,
    configuredIds,
    loadCatalog,
    getTemplate: getCustomProviderTemplate,
    read,
    create,
    update,
    remove,
    saveDraft,
  };
}
