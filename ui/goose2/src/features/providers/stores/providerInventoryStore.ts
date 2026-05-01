import { create } from "zustand";
import type { ProviderInventoryEntryDto } from "@aaif/goose-sdk";
import { perfLog } from "@/shared/lib/perfLog";

export interface ProviderInventoryState {
  entries: Map<string, ProviderInventoryEntryDto>;
  loading: boolean;
}

interface ProviderInventoryActions {
  setEntries: (entries: ProviderInventoryEntryDto[]) => void;
  mergeEntries: (entries: ProviderInventoryEntryDto[]) => void;
  setLoading: (loading: boolean) => void;
}

export type ProviderInventoryStore = ProviderInventoryState &
  ProviderInventoryActions;

export const useProviderInventoryStore = create<ProviderInventoryStore>(
  (set) => ({
    entries: new Map(),
    loading: false,

    setEntries: (entries) => {
      const map = new Map<string, ProviderInventoryEntryDto>();
      for (const entry of entries) {
        map.set(entry.providerId, entry);
      }
      set({ entries: map });
      perfLog(
        `[perf:inventory] setEntries n=${entries.length} providers=[${entries.map((e) => e.providerId).join(",")}]`,
      );
    },

    mergeEntries: (entries) => {
      set((state) => {
        const map = new Map(state.entries);
        for (const entry of entries) {
          map.set(entry.providerId, entry);
        }
        return { entries: map };
      });
    },

    setLoading: (loading) => set({ loading }),
  }),
);
