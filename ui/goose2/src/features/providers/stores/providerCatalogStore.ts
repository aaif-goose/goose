import { create } from "zustand";
import type { ProviderCatalogEntry } from "@/shared/types/providers";

function normalizeProviderKey(value: string): string {
  return value
    .toLowerCase()
    .split(/[-_\s]+/)
    .filter(Boolean)
    .join("_");
}

function buildEntriesById(
  entries: ProviderCatalogEntry[],
): Map<string, ProviderCatalogEntry> {
  const entriesById = new Map<string, ProviderCatalogEntry>();
  for (const entry of entries) {
    entriesById.set(entry.id, entry);
  }
  return entriesById;
}

function buildAgentAliasMap(
  entries: ProviderCatalogEntry[],
): Map<string, string> {
  const aliasMap = new Map<string, string>();
  for (const entry of entries) {
    if (entry.category !== "agent") {
      continue;
    }

    aliasMap.set(normalizeProviderKey(entry.id), entry.id);
    for (const alias of entry.aliases ?? []) {
      aliasMap.set(normalizeProviderKey(alias), entry.id);
    }
  }
  return aliasMap;
}

export interface ProviderCatalogState {
  entries: ProviderCatalogEntry[];
  entriesById: Map<string, ProviderCatalogEntry>;
  agentAliasMap: Map<string, string>;
  loading: boolean;
  loaded: boolean;
  error: string | null;
}

interface ProviderCatalogActions {
  load: () => Promise<ProviderCatalogEntry[]>;
  setEntries: (entries: ProviderCatalogEntry[]) => void;
  reset: () => void;
}

export type ProviderCatalogStore = ProviderCatalogState &
  ProviderCatalogActions;

let loadPromise: Promise<ProviderCatalogEntry[]> | null = null;

const EMPTY_STATE: ProviderCatalogState = {
  entries: [],
  entriesById: new Map(),
  agentAliasMap: new Map(),
  loading: false,
  loaded: false,
  error: null,
};

export const useProviderCatalogStore = create<ProviderCatalogStore>(
  (set, get) => ({
    ...EMPTY_STATE,

    load: async () => {
      if (loadPromise) {
        return loadPromise;
      }

      const current = get();
      if (current.loaded) {
        return current.entries;
      }

      set({ loading: true, error: null });
      loadPromise = import("../api/catalog")
        .then(({ listProviderSetupCatalog }) => listProviderSetupCatalog())
        .then((entries) => {
          get().setEntries(entries);
          return entries;
        })
        .catch((error: unknown) => {
          const message =
            error instanceof Error ? error.message : "Failed to load catalog";
          set({ loading: false, loaded: false, error: message });
          throw error;
        })
        .finally(() => {
          loadPromise = null;
        });

      return loadPromise;
    },

    setEntries: (entries) => {
      set({
        entries,
        entriesById: buildEntriesById(entries),
        agentAliasMap: buildAgentAliasMap(entries),
        loading: false,
        loaded: true,
        error: null,
      });
    },

    reset: () => {
      loadPromise = null;
      set({
        ...EMPTY_STATE,
        entriesById: new Map(),
        agentAliasMap: new Map(),
      });
    },
  }),
);
