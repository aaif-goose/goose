import { load, type Store } from "@tauri-apps/plugin-store";
import type { ChatTab, SectionId } from "../types";

const STORE_FILE = "state.json";
const UI_KEY = "ui";

export interface PersistedUiState {
  tabs: ChatTab[];
  activeTab: string;
  section: SectionId;
  leftCollapsed: boolean;
  rightCollapsed: boolean;
  openNote: string | null;
}

let storePromise: Promise<Store> | null = null;

function getStore(): Promise<Store> {
  if (!storePromise) storePromise = load(STORE_FILE, { autoSave: false });
  return storePromise;
}

/**
 * Strip transient runtime state before writing to disk:
 * - `streaming` flags on messages (we never resume in streaming state)
 * - `gooseSessionId` on tabs (ACP sessions don't survive app restart)
 */
function cleanTabs(tabs: ChatTab[]): ChatTab[] {
  return tabs.map((t) => ({
    ...t,
    gooseSessionId: undefined,
    messages: t.messages.map((m) => (m.streaming ? { ...m, streaming: false } : m)),
  }));
}

export async function loadUiState(): Promise<PersistedUiState | null> {
  try {
    const store = await getStore();
    const raw = await store.get<PersistedUiState>(UI_KEY);
    if (!raw) return null;
    // Basic shape check — older schemas return null so we start fresh.
    if (!Array.isArray(raw.tabs) || typeof raw.activeTab !== "string") return null;
    return raw;
  } catch (err) {
    console.warn("[persistence] load failed", err);
    return null;
  }
}

export async function saveUiState(state: PersistedUiState): Promise<void> {
  try {
    const store = await getStore();
    await store.set(UI_KEY, { ...state, tabs: cleanTabs(state.tabs) });
    await store.save();
  } catch (err) {
    console.warn("[persistence] save failed", err);
  }
}
