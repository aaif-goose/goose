const HOME_SESSION_STORAGE_KEY = "goose:home-session-id";

export function loadStoredHomeSessionId(): string | null {
  if (typeof window === "undefined") {
    return null;
  }
  try {
    return window.localStorage.getItem(HOME_SESSION_STORAGE_KEY);
  } catch {
    return null;
  }
}

export function persistHomeSessionId(sessionId: string | null): void {
  if (typeof window === "undefined") {
    return;
  }
  try {
    if (sessionId) {
      window.localStorage.setItem(HOME_SESSION_STORAGE_KEY, sessionId);
      return;
    }
    window.localStorage.removeItem(HOME_SESSION_STORAGE_KEY);
  } catch {
    // localStorage may be unavailable
  }
}
