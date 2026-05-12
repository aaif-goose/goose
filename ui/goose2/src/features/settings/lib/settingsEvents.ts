export const OPEN_SETTINGS_EVENT = "goose:open-settings";

export function requestOpenSettings(section?: string) {
  if (typeof window === "undefined") {
    return;
  }

  window.dispatchEvent(
    new CustomEvent(OPEN_SETTINGS_EVENT, {
      detail: section ? { section } : undefined,
    }),
  );
}
