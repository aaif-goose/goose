declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export {};
