import { create } from "zustand";
import type { ProviderCatalogEntry } from "@/shared/types/providers";

export const GOOSE_PROVIDER_CATALOG_ENTRY: ProviderCatalogEntry = {
  id: "goose",
  displayName: "Goose",
  category: "agent",
  description: "Block's open-source coding agent",
  setupMethod: "none",
  group: "default",
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

export interface ProviderCatalogState {
  entries: ProviderCatalogEntry[];
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

function emptyState(): ProviderCatalogState {
  return {
    entries: [GOOSE_PROVIDER_CATALOG_ENTRY],
    loading: false,
    loaded: false,
    error: null,
  };
}

export const useProviderCatalogStore = create<ProviderCatalogStore>(
  (set, get) => ({
    ...emptyState(),

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
        loading: false,
        loaded: true,
        error: null,
      });
    },

    reset: () => {
      loadPromise = null;
      set(emptyState());
    },
  }),
);
