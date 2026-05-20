import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ThemeProvider, useTheme } from "./ThemeProvider";

const { mockCreateThemeVars, mockExtractThemeInfo, mockLoadThemeData } =
  vi.hoisted(() => ({
    mockCreateThemeVars: vi.fn(),
    mockExtractThemeInfo: vi.fn(),
    mockLoadThemeData: vi.fn(),
  }));

vi.mock("./adaptive-theme", () => ({
  createThemeVars: mockCreateThemeVars,
  hexToHsl: vi.fn((hex: string) => {
    const normalized = hex.toLowerCase();
    if (normalized === "#ffffff") {
      return "0 0% 100%";
    }
    if (normalized === "#000000") {
      return "0 0% 0%";
    }
    if (normalized === "#ef4444") {
      return "0 84.2% 60.2%";
    }
    return "217.2 91.2% 59.8%";
  }),
}));

vi.mock("./theme-loader", () => ({
  extractThemeInfo: mockExtractThemeInfo,
  isSyntaxThemeName: (value: string | null) =>
    value === "houston" || value === "github-light" || value === "dracula",
  loadThemeData: mockLoadThemeData,
}));

function createDeferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });

  return { promise, resolve, reject };
}

function createMediaQueryList(initialMatches: boolean) {
  let matches = initialMatches;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();

  const mediaQuery = {
    get matches() {
      return matches;
    },
    media: "(prefers-color-scheme: dark)",
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(
      (_event: string, listener: (event: MediaQueryListEvent) => void) => {
        listeners.add(listener);
      },
    ),
    removeEventListener: vi.fn(
      (_event: string, listener: (event: MediaQueryListEvent) => void) => {
        listeners.delete(listener);
      },
    ),
    dispatchEvent: vi.fn(),
  };

  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: vi.fn(() => mediaQuery),
  });

  return {
    setMatches(nextMatches: boolean) {
      matches = nextMatches;
      const event = { matches: nextMatches } as MediaQueryListEvent;
      for (const listener of listeners) {
        listener(event);
      }
    },
  };
}

function ThemeConsumer() {
  const {
    selectedThemeName,
    usingSystemTheme,
    resolvedThemeName,
    isDark,
    isLoading,
    accentColor,
    density,
    setTheme,
    setAccentColor,
    setDensity,
  } = useTheme();

  return (
    <div>
      <span data-testid="selected-theme">{selectedThemeName ?? "system"}</span>
      <span data-testid="using-system">{String(usingSystemTheme)}</span>
      <span data-testid="resolved-theme">{resolvedThemeName}</span>
      <span data-testid="is-dark">{String(isDark)}</span>
      <span data-testid="is-loading">{String(isLoading)}</span>
      <span data-testid="accent">{accentColor}</span>
      <span data-testid="density">{density}</span>
      <button onClick={() => setTheme("dracula")} type="button">
        Set Dracula
      </button>
      <button onClick={() => setTheme(null)} type="button">
        Use System
      </button>
      <button onClick={() => setAccentColor("#ef4444")} type="button">
        Set Red Accent
      </button>
      <button onClick={() => setDensity("compact")} type="button">
        Set Compact
      </button>
    </div>
  );
}

describe("ThemeProvider", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.className = "";
    document.documentElement.removeAttribute("style");

    mockLoadThemeData.mockResolvedValue({});
    mockExtractThemeInfo.mockImplementation((name: string) => ({
      name,
      bg: name === "github-light" ? "#ffffff" : "#111827",
      fg: name === "github-light" ? "#111827" : "#f9fafb",
      comment: name === "github-light" ? "#6b7280" : "#94a3b8",
      added: "#22c55e",
      deleted: "#ef4444",
      modified: "#f59e0b",
    }));
    mockCreateThemeVars.mockImplementation((bg: string) => ({
      isDark: bg !== "#ffffff",
      vars: {
        "--background": bg === "#ffffff" ? "0 0% 100%" : "224 71% 4%",
        "--foreground": bg === "#ffffff" ? "224 71% 4%" : "0 0% 100%",
        "--sidebar-background": bg === "#ffffff" ? "0 0% 98%" : "224 71% 6%",
        "--ui-warning": "#f59e0b",
      },
    }));
  });

  it("defaults to system mode and resolves through the OS preference", async () => {
    createMediaQueryList(false);

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("selected-theme")).toHaveTextContent("system");
    expect(screen.getByTestId("using-system")).toHaveTextContent("true");
    expect(screen.getByTestId("resolved-theme")).toHaveTextContent(
      "github-light",
    );

    await waitFor(() => {
      expect(screen.getByTestId("is-loading")).toHaveTextContent("false");
    });

    expect(document.documentElement).toHaveClass("light");
    expect(localStorage.getItem("goose-custom-theme")).toBeNull();
  });

  it("reacts to system theme changes while no explicit theme is selected", async () => {
    const media = createMediaQueryList(false);

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("resolved-theme")).toHaveTextContent(
        "github-light",
      );
    });

    media.setMatches(true);

    await waitFor(() => {
      expect(screen.getByTestId("resolved-theme")).toHaveTextContent(
        "github-dark",
      );
    });

    expect(document.documentElement).toHaveClass("dark");
  });

  it("switches to an explicit theme and persists it", async () => {
    const user = userEvent.setup();
    createMediaQueryList(false);

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await user.click(screen.getByRole("button", { name: "Set Dracula" }));

    await waitFor(() => {
      expect(screen.getByTestId("selected-theme")).toHaveTextContent("dracula");
      expect(screen.getByTestId("using-system")).toHaveTextContent("false");
      expect(screen.getByTestId("resolved-theme")).toHaveTextContent("dracula");
    });

    expect(localStorage.getItem("goose-custom-theme")).toBe("dracula");
  });

  it("returns to system mode when the theme is cleared", async () => {
    const user = userEvent.setup();
    localStorage.setItem("goose-custom-theme", "dracula");
    createMediaQueryList(true);

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("selected-theme")).toHaveTextContent("dracula");
    });

    await user.click(screen.getByRole("button", { name: "Use System" }));

    await waitFor(() => {
      expect(screen.getByTestId("selected-theme")).toHaveTextContent("system");
      expect(screen.getByTestId("resolved-theme")).toHaveTextContent(
        "github-dark",
      );
    });

    expect(localStorage.getItem("goose-custom-theme")).toBeNull();
  });

  it("migrates legacy preset storage to system mode", async () => {
    localStorage.setItem("goose-theme", "dark");
    createMediaQueryList(false);

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("selected-theme")).toHaveTextContent("system");
      expect(screen.getByTestId("resolved-theme")).toHaveTextContent(
        "github-light",
      );
    });
  });

  it("migrates a legacy named theme into an explicit selection", async () => {
    localStorage.setItem("goose-theme", "dracula");
    createMediaQueryList(false);

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("selected-theme")).toHaveTextContent("dracula");
      expect(screen.getByTestId("resolved-theme")).toHaveTextContent("dracula");
    });
  });

  it("applies cached theme vars before the async theme load resolves", () => {
    localStorage.setItem(
      "goose-theme-cache",
      JSON.stringify({
        isDark: true,
        resolvedThemeName: "dracula",
        vars: {
          "--background": "240 10% 4%",
          "--foreground": "0 0% 100%",
        },
      }),
    );
    mockLoadThemeData.mockReturnValue(new Promise(() => {}));
    createMediaQueryList(false);

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    expect(document.documentElement).toHaveClass("dark");
    expect(
      document.documentElement.style.getPropertyValue("--background"),
    ).toBe("240 10% 4%");
    expect(screen.getByTestId("is-dark")).toHaveTextContent("true");
    expect(screen.getByTestId("is-loading")).toHaveTextContent("true");
  });

  it("only applies accent overrides after a theme is selected", async () => {
    const user = userEvent.setup();
    createMediaQueryList(false);

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("is-loading")).toHaveTextContent("false");
    });

    await user.click(screen.getByRole("button", { name: "Set Red Accent" }));
    await user.click(screen.getByRole("button", { name: "Set Compact" }));

    expect(localStorage.getItem("goose-accent-color")).toBe("#ef4444");
    expect(localStorage.getItem("goose-density")).toBe("compact");
    expect(document.documentElement.style.getPropertyValue("--primary")).toBe(
      "",
    );

    await user.click(screen.getByRole("button", { name: "Set Dracula" }));

    await waitFor(() => {
      expect(screen.getByTestId("selected-theme")).toHaveTextContent("dracula");
    });

    expect(document.documentElement.style.getPropertyValue("--primary")).toBe(
      "0 84.2% 60.2%",
    );
    expect(
      document.documentElement.style.getPropertyValue("--primary-foreground"),
    ).toBe("0 0% 100%");
    expect(
      document.documentElement.style.getPropertyValue("--brand-color"),
    ).toBe("#ef4444");
    expect(document.documentElement.dataset.density).toBe("compact");
  });

  it("falls back to the built-in system theme when async theme loading fails", async () => {
    createMediaQueryList(false);
    mockLoadThemeData.mockRejectedValueOnce(new Error("chunk load failed"));

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("is-loading")).toHaveTextContent("false");
    });

    expect(screen.getByTestId("resolved-theme")).toHaveTextContent(
      "github-light",
    );
    expect(screen.getByTestId("is-dark")).toHaveTextContent("false");
    expect(document.documentElement).toHaveClass("light");
    expect(
      document.documentElement.style.getPropertyValue("--background"),
    ).not.toBe("");
  });

  it("ignores stale theme loads that finish after a newer request", async () => {
    const user = userEvent.setup();
    createMediaQueryList(false);

    const githubLight = createDeferred<Record<string, never>>();
    const dracula = createDeferred<Record<string, never>>();

    mockLoadThemeData.mockImplementation((themeName: string) => {
      if (themeName === "github-light") {
        return githubLight.promise;
      }

      if (themeName === "dracula") {
        return dracula.promise;
      }

      return Promise.resolve({});
    });

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await user.click(screen.getByRole("button", { name: "Set Dracula" }));

    dracula.resolve({});

    await waitFor(() => {
      expect(screen.getByTestId("resolved-theme")).toHaveTextContent("dracula");
      expect(document.documentElement).toHaveClass("dark");
    });

    githubLight.resolve({});

    await Promise.resolve();
    await Promise.resolve();

    expect(screen.getByTestId("resolved-theme")).toHaveTextContent("dracula");
    expect(document.documentElement).toHaveClass("dark");
    expect(
      JSON.parse(localStorage.getItem("goose-theme-cache") ?? "{}"),
    ).toMatchObject({
      resolvedThemeName: "dracula",
    });
  });
});
