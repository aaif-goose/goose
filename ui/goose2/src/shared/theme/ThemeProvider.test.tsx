import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, beforeEach } from "vitest";
import { ThemeProvider, useTheme } from "./ThemeProvider";

function ThemeConsumer() {
  const {
    theme,
    setTheme,
    accentColor,
    accentColorPreference,
    setAccentColor,
    resetAccentColor,
    density,
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
    </div>
  );
}

describe("ThemeProvider", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.classList.remove("light", "dark");
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

  it("provides default density", () => {
    render(
      <ThemeProvider>
        <ThemeConsumer />
      </ThemeProvider>,
    );
    expect(screen.getByTestId("density")).toHaveTextContent("comfortable");
  });
});
