import { useEffect } from "react";

const KEY = "goose-zoom-level";
const STEP = 0.1;
const MIN = 0.7;
const MAX = 1.3;

function adjust(n: number) {
  return Math.round(Math.min(MAX, Math.max(MIN, n)) * 100) / 100;
}

function getStored(): number {
  const v = Number.parseFloat(localStorage.getItem(KEY) ?? "");
  return Number.isNaN(v) ? 1.0 : adjust(v);
}

async function applyZoom(level: number) {
  if (!window.__TAURI_INTERNALS__) return;
  try {
    const { getCurrentWebviewWindow } = await import(
      "@tauri-apps/api/webviewWindow"
    );
    await getCurrentWebviewWindow().setZoom(level);
  } catch {
    // non-Tauri environment
  }
}

export function useZoom() {
  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;

    let level = getStored();
    applyZoom(level);

    const handler = (e: KeyboardEvent) => {
      if (!e.metaKey && !e.ctrlKey) return;

      if (e.key === "=" || e.key === "+") level = adjust(level + STEP);
      else if (e.key === "-") level = adjust(level - STEP);
      else if (e.key === "0") level = adjust(1.0);
      else return;

      e.preventDefault();
      localStorage.setItem(KEY, String(level));
      applyZoom(level);
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);
}
