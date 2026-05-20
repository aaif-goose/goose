import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { renderWithProviders } from "@/test/render";

import { AppearanceSettings } from "../AppearanceSettings";

const { mockUseTheme } = vi.hoisted(() => ({
  mockUseTheme: vi.fn(),
}));

vi.mock("@/shared/theme/ThemeProvider", async () => {
  const actual = await vi.importActual<
    typeof import("@/shared/theme/ThemeProvider")
  >("@/shared/theme/ThemeProvider");

  return {
    ...actual,
    ACCENT_COLORS: [
      { name: "blue", value: "#3b82f6" },
      { name: "red", value: "#ef4444" },
    ],
    useTheme: mockUseTheme,
  };
});

vi.mock("@/shared/theme/theme-loader", () => ({
  SYNTAX_THEMES: ["github-light", "dracula", "houston"],
  isLightTheme: (name: string) => name === "github-light",
}));

describe("AppearanceSettings", () => {
  const baseThemeState = {
    selectedThemeName: "houston",
    usingSystemTheme: false,
    setTheme: vi.fn(),
    accentColor: "#3b82f6",
    setAccentColor: vi.fn(),
    density: "comfortable",
    setDensity: vi.fn(),
    resolvedThemeName: "houston",
    isDark: true,
    isLoading: false,
  } as const;

  beforeEach(() => {
    HTMLElement.prototype.scrollIntoView = vi.fn();
    mockUseTheme.mockReturnValue({
      ...baseThemeState,
      setTheme: vi.fn(),
      setAccentColor: vi.fn(),
      setDensity: vi.fn(),
    });
  });

  it("lets the user return to system mode", async () => {
    const user = userEvent.setup();
    const setTheme = vi.fn();
    mockUseTheme.mockReturnValue({
      ...baseThemeState,
      setTheme,
      setAccentColor: vi.fn(),
      setDensity: vi.fn(),
    });

    renderWithProviders(<AppearanceSettings />);

    await user.click(screen.getByTestId("theme-option-system"));
    expect(setTheme).toHaveBeenCalledWith(null);
  });

  it("filters themes from the search input", async () => {
    const user = userEvent.setup();

    renderWithProviders(<AppearanceSettings />);

    await user.type(screen.getByTestId("theme-search-input"), "git");

    expect(screen.getByTestId("theme-option-github-light")).toBeVisible();
    expect(
      screen.queryByTestId("theme-option-dracula"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("theme-option-houston"),
    ).not.toBeInTheDocument();
  });

  it("selects a theme from the picker", async () => {
    const user = userEvent.setup();
    const setTheme = vi.fn();
    mockUseTheme.mockReturnValue({
      ...baseThemeState,
      setTheme,
      setAccentColor: vi.fn(),
      setDensity: vi.fn(),
    });

    renderWithProviders(<AppearanceSettings />);

    await user.click(screen.getByTestId("theme-option-dracula"));
    expect(setTheme).toHaveBeenCalledWith("dracula");
  });

  it("updates the accent color when a theme is selected", async () => {
    const user = userEvent.setup();
    const setAccentColor = vi.fn();
    mockUseTheme.mockReturnValue({
      ...baseThemeState,
      setTheme: vi.fn(),
      setAccentColor,
      setDensity: vi.fn(),
    });

    renderWithProviders(<AppearanceSettings />);

    await user.click(screen.getByTestId("accent-color-red"));
    expect(setAccentColor).toHaveBeenCalledWith("#ef4444");
  });

  it("disables accent controls while using system mode", async () => {
    const user = userEvent.setup();
    const setAccentColor = vi.fn();
    mockUseTheme.mockReturnValue({
      ...baseThemeState,
      selectedThemeName: null,
      usingSystemTheme: true,
      setTheme: vi.fn(),
      setAccentColor,
      setDensity: vi.fn(),
    });

    renderWithProviders(<AppearanceSettings />);

    const accentButton = screen.getByTestId("accent-color-red");
    expect(accentButton).toBeDisabled();

    await user.click(accentButton);
    expect(setAccentColor).not.toHaveBeenCalled();
  });
});
