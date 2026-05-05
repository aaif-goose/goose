import { create } from "zustand";
import type { ProviderCatalogEntry } from "@/shared/types/providers";
import { normalizeProviderKey } from "../lib/providerKey";

export const GOOSE_PROVIDER_CATALOG_ENTRY: ProviderCatalogEntry = {
  id: "goose",
  displayName: "Goose",
  category: "agent",
  description: "Block's open-source coding agent",
  setupMethod: "none",
  tier: "promoted",
  aliases: ["goose"],
};

function withGooseFallback(
  entries: ProviderCatalogEntry[],
): ProviderCatalogEntry[] {
  if (entries.some((entry) => entry.id === GOOSE_PROVIDER_CATALOG_ENTRY.id)) {
    return entries;
  }
  return [GOOSE_PROVIDER_CATALOG_ENTRY, ...entries];
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
  entries: [GOOSE_PROVIDER_CATALOG_ENTRY],
  entriesById: buildEntriesById([GOOSE_PROVIDER_CATALOG_ENTRY]),
  agentAliasMap: buildAgentAliasMap([GOOSE_PROVIDER_CATALOG_ENTRY]),
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
      const nextEntries = withGooseFallback(entries);
      set({
        entries: nextEntries,
        entriesById: buildEntriesById(nextEntries),
        agentAliasMap: buildAgentAliasMap(nextEntries),
        loading: false,
        loaded: true,
        error: null,
      });
    },

    reset: () => {
      loadPromise = null;
      set({
        ...EMPTY_STATE,
        entries: [GOOSE_PROVIDER_CATALOG_ENTRY],
        entriesById: buildEntriesById([GOOSE_PROVIDER_CATALOG_ENTRY]),
        agentAliasMap: buildAgentAliasMap([GOOSE_PROVIDER_CATALOG_ENTRY]),
      });
    },
  }),
);
