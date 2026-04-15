import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";

const mockSetZoom = vi.fn<(factor: number) => Promise<void>>();

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ setZoom: mockSetZoom }),
}));

import { useZoom } from "../useZoom";

function fireKey(key: string, opts: Partial<KeyboardEventInit> = {}) {
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key, bubbles: true, ...opts }),
  );
}

describe("useZoom", () => {
  beforeEach(() => {
    localStorage.clear();
    mockSetZoom.mockClear();
    mockSetZoom.mockResolvedValue(undefined);
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = true;
  });

  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("applies stored zoom on mount", async () => {
    localStorage.setItem("goose-zoom-level", "1.3");
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.3));
  });

  it("defaults to 1.0 when nothing stored", async () => {
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.0));
  });

  it("clamps invalid stored values", async () => {
    localStorage.setItem("goose-zoom-level", "10");
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.3));
  });

  it("falls back to 1.0 for garbage stored value", async () => {
    localStorage.setItem("goose-zoom-level", "garbage");
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.0));
  });

  it("Cmd+= zooms in", async () => {
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalled());
    mockSetZoom.mockClear();
    fireKey("=", { metaKey: true });
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.1));
    expect(localStorage.getItem("goose-zoom-level")).toBe("1.1");
  });

  it("Cmd+- zooms out", async () => {
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalled());
    mockSetZoom.mockClear();
    fireKey("-", { metaKey: true });
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(0.9));
  });

  it("Cmd+0 resets to 1.0", async () => {
    localStorage.setItem("goose-zoom-level", "1.3");
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.3));
    mockSetZoom.mockClear();
    fireKey("0", { metaKey: true });
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.0));
  });

  it("Ctrl+= works (non-mac)", async () => {
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalled());
    mockSetZoom.mockClear();
    fireKey("=", { ctrlKey: true });
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.1));
  });

  it("ignores keys without modifier", async () => {
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalled());
    mockSetZoom.mockClear();
    fireKey("=");
    fireKey("-");
    fireKey("0");
    await new Promise((r) => setTimeout(r, 50));
    expect(mockSetZoom).not.toHaveBeenCalled();
  });

  it("clamps at min boundary", async () => {
    localStorage.setItem("goose-zoom-level", "0.7");
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(0.7));
    mockSetZoom.mockClear();
    fireKey("-", { metaKey: true });
    // Still 0.7 — clamped, doesn't go lower
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(0.7));
  });

  it("clamps at max boundary", async () => {
    localStorage.setItem("goose-zoom-level", "1.3");
    renderHook(() => useZoom());
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.3));
    mockSetZoom.mockClear();
    fireKey("=", { metaKey: true });
    // Still 1.3 — clamped, doesn't go higher
    await vi.waitFor(() => expect(mockSetZoom).toHaveBeenCalledWith(1.3));
  });

  it("does not call Tauri API without __TAURI_INTERNALS__", async () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    mockSetZoom.mockClear();
    renderHook(() => useZoom());
    fireKey("=", { metaKey: true });
    await new Promise((r) => setTimeout(r, 50));
    expect(mockSetZoom).not.toHaveBeenCalled();
  });

  it("cleans up listener on unmount", () => {
    const spy = vi.spyOn(window, "removeEventListener");
    const { unmount } = renderHook(() => useZoom());
    unmount();
    expect(spy).toHaveBeenCalledWith("keydown", expect.any(Function));
    spy.mockRestore();
  });
});
