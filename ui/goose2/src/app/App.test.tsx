import { render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { ThemeProvider } from "@/shared/theme/ThemeProvider";

vi.mock("@/app/AppShell", () => ({
  AppShell: () => <div data-testid="app-shell" />,
}));

describe("App", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("prevents default window navigation when files are dragged into the app", () => {
    vi.stubGlobal("__TAURI_INTERNALS__", undefined);
    vi.stubGlobal(
      "DragEvent",
      window.DragEvent ?? class DragEvent extends Event {},
    );

    render(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    const dragOverEvent = new DragEvent("dragover", {
      bubbles: true,
      cancelable: true,
    });
    const dropEvent = new DragEvent("drop", {
      bubbles: true,
      cancelable: true,
    });

    window.dispatchEvent(dragOverEvent);
    window.dispatchEvent(dropEvent);

    expect(dragOverEvent.defaultPrevented).toBe(true);
    expect(dropEvent.defaultPrevented).toBe(true);
  });
});
