const DRAFTS_STORAGE_KEY = "goose:chat-drafts";

export function loadCachedDrafts(): Record<string, string> {
  if (typeof window === "undefined") return {};
  try {
    const stored = window.localStorage.getItem(DRAFTS_STORAGE_KEY);
    if (!stored) return {};
    const parsed = JSON.parse(stored);
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export function persistDrafts(drafts: Record<string, string>): void {
  if (typeof window === "undefined") return;
  try {
    const nonEmpty = Object.fromEntries(
      Object.entries(drafts).filter(([, v]) => v.length > 0),
    );
    if (Object.keys(nonEmpty).length === 0) {
      window.localStorage.removeItem(DRAFTS_STORAGE_KEY);
    } else {
      window.localStorage.setItem(DRAFTS_STORAGE_KEY, JSON.stringify(nonEmpty));
    }
  } catch {
    // localStorage may be unavailable
  }
}
