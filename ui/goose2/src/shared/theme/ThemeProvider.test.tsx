import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect, beforeEach } from "vitest";
import { ThemeProvider, useTheme } from "./ThemeProvider";

const testDirname = dirname(fileURLToPath(import.meta.url));

function rootCssVariable(name: string) {
  return getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
}

function ThemeConsumer() {
  const {
    theme,
    setTheme,
    accentColor,
    accentColorPreference,
    setAccentColor,
    resetAccentColor,
    density,
    setDensity,
  } = useTheme();
  return (
    <div>
      <span data-testid="theme">{theme}</span>
      <span data-testid="accent">{accentColor}</span>
      <span data-testid="accent-preference">{accentColorPreference}</span>
      <span data-testid="density">{density}</span>
      <button type="button" onClick={() => setTheme("dark")}>
        Set Dark
      </button>
      <button type="button" onClick={() => setTheme("light")}>
        Set Light
      </button>
      <button type="button" onClick={() => setAccentColor("#f97316")}>
        Set Orange
      </button>
      <button type="button" onClick={() => setAccentColor("#fff")}>
        Set White
      </button>
      <button type="button" onClick={() => setAccentColor("red")}>
        Set Invalid
      </button>
      <button type="button" onClick={resetAccentColor}>
        Reset Accent
      </button>
      <button type="button" onClick={() => setDensity("spacious")}>
        Set Spacious
      </button>
    </div>
  );
}

describe("ThemeProvider", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.classList.remove("light", "dark");
    document.documentElement.removeAttribute("data-density");
    document.documentElement.removeAttribute("style");
  });

  it("provides default theme as system", () => {
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("theme")).toHaveTextContent("system");
  });

  it("switches to dark theme", async () => {
    const user = userEvent.setup();
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );
    await user.click(screen.getByText("Set Dark"));
    expect(screen.getByTestId("theme")).toHaveTextContent("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("persists theme to localStorage", async () => {
    const user = userEvent.setup();
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );
    await user.click(screen.getByText("Set Light"));
    expect(localStorage.getItem("goose-theme")).toBe("light");
  });

  it("provides default accent color", () => {
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("accent")).toHaveTextContent("#1a1a1a");
    expect(screen.getByTestId("accent-preference")).toHaveTextContent(
      "default",
    );
  });

  it("applies accent color tokens to the document", async () => {
    const user = userEvent.setup();
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await user.click(screen.getByText("Set Orange"));

    expect(localStorage.getItem("goose-accent-color")).toBe("#f97316");
    expect(document.documentElement.style.getPropertyValue("--brand")).toBe(
      "#f97316",
    );
    expect(
      document.documentElement.style.getPropertyValue("--brand-foreground"),
    ).toBe("#000000");
    expect(
      document.documentElement.style.getPropertyValue("--color-brand"),
    ).toBe("#f97316");
    expect(
      document.documentElement.style.getPropertyValue(
        "--color-brand-foreground",
      ),
    ).toBe("#000000");
    expect(document.documentElement.style.accentColor).toBe(
      "rgb(249, 115, 22)",
    );
  });

  it("normalizes and validates accent colors", async () => {
    const user = userEvent.setup();
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await user.click(screen.getByText("Set White"));

    expect(localStorage.getItem("goose-accent-color")).toBe("#ffffff");
    expect(
      document.documentElement.style.getPropertyValue("--color-brand"),
    ).toBe("#ffffff");
    expect(
      document.documentElement.style.getPropertyValue(
        "--color-brand-foreground",
      ),
    ).toBe("#000000");

    await user.click(screen.getByText("Set Invalid"));

    expect(localStorage.getItem("goose-accent-color")).toBeNull();
    expect(screen.getByTestId("accent-preference")).toHaveTextContent(
      "default",
    );
    expect(
      document.documentElement.style.getPropertyValue("--color-brand"),
    ).toBe("#1a1a1a");
  });

  it("resets custom accent colors to the theme default", async () => {
    const user = userEvent.setup();
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await user.click(screen.getByText("Set Orange"));
    await user.click(screen.getByText("Reset Accent"));

    expect(localStorage.getItem("goose-accent-color")).toBeNull();
    expect(screen.getByTestId("accent")).toHaveTextContent("#1a1a1a");
    expect(screen.getByTestId("accent-preference")).toHaveTextContent(
      "default",
    );
    expect(
      document.documentElement.style.getPropertyValue("--color-brand"),
    ).toBe("#1a1a1a");
  });

  it("updates the default accent color with the theme", async () => {
    const user = userEvent.setup();
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await user.click(screen.getByText("Set Dark"));

    expect(screen.getByTestId("accent")).toHaveTextContent("#ffffff");
    expect(
      document.documentElement.style.getPropertyValue("--color-brand"),
    ).toBe("#ffffff");
    expect(
      document.documentElement.style.getPropertyValue(
        "--color-brand-foreground",
      ),
    ).toBe("#000000");
  });

  it("falls back to default accent color when storage is invalid", () => {
    localStorage.setItem("goose-accent-color", "blue");

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("accent")).toHaveTextContent("#1a1a1a");
    expect(screen.getByTestId("accent-preference")).toHaveTextContent(
      "default",
    );
    expect(rootCssVariable("--color-brand")).toBe("#1a1a1a");
    expect(rootCssVariable("--color-brand-foreground")).toBe("#ffffff");
  });

  it("provides default density", () => {
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("density")).toHaveTextContent("comfortable");
    expect(document.documentElement).not.toHaveAttribute("data-density");
    expect(
      document.documentElement.style.getPropertyValue("--density-spacing"),
    ).toBe("");
    expect(document.documentElement.style.getPropertyValue("--spacing")).toBe(
      "",
    );
  });

  it("falls back to default theme when storage is invalid", () => {
    localStorage.setItem("goose-theme", "sepia");

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("theme")).toHaveTextContent("system");
  });

  it("reads persisted density", () => {
    localStorage.setItem("goose-density", "compact");

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("density")).toHaveTextContent("compact");
    expect(document.documentElement.dataset.density).toBe("compact");
    expect(
      document.documentElement.style.getPropertyValue("--density-spacing"),
    ).toBe("");
    expect(document.documentElement.style.getPropertyValue("--spacing")).toBe(
      "",
    );
  });

  it("persists density and updates spacing tokens", async () => {
    const user = userEvent.setup();
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    await user.click(screen.getByText("Set Spacious"));

    expect(screen.getByTestId("density")).toHaveTextContent("spacious");
    expect(localStorage.getItem("goose-density")).toBe("spacious");
    expect(document.documentElement.dataset.density).toBe("spacious");
    expect(
      document.documentElement.style.getPropertyValue("--density-spacing"),
    ).toBe("");
    expect(document.documentElement.style.getPropertyValue("--spacing")).toBe(
      "",
    );
  });

  it("falls back to comfortable density when storage is invalid", () => {
    localStorage.setItem("goose-density", "tiny");

    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("density")).toHaveTextContent("comfortable");
    expect(document.documentElement).not.toHaveAttribute("data-density");
  });

  it("keeps density spacing values in CSS", () => {
    const css = readFileSync(
      resolve(testDirname, "../styles/globals.css"),
      "utf8",
    );

    expect(css).toContain('[data-density="compact"]');
    expect(css).toContain("--density-spacing: 0.75;");
    expect(css).toContain("--spacing: 0.1875rem;");
    expect(css).toContain('[data-density="spacious"]');
    expect(css).toContain("--density-spacing: 1.25;");
    expect(css).toContain("--spacing: 0.3125rem;");
    expect(css).toContain("padding: calc(0.5rem * var(--density-spacing));");
  });
});
