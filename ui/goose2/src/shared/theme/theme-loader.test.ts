import { describe, expect, it } from "vitest";
import type { ThemeRegistrationRaw } from "shiki";

import { extractThemeInfo, isLightTheme } from "./theme-loader";

describe("theme-loader", () => {
  it("extracts theme metadata including comment and git colors", () => {
    const theme = {
      colors: {
        "editor.background": "#0f172a",
        "editor.foreground": "#e2e8f0",
        "gitDecoration.addedResourceForeground": "#22c55eff",
        "editorGutter.deletedBackground": "#ef4444cc",
        "gitDecoration.modifiedResourceForeground": "#f59e0b",
      },
      settings: [
        {
          scope: ["comment", "punctuation.definition.comment"],
          settings: { foreground: "#94a3b8" },
        },
      ],
    } satisfies ThemeRegistrationRaw;

    expect(extractThemeInfo("dracula", theme)).toEqual({
      name: "dracula",
      bg: "#0f172a",
      fg: "#e2e8f0",
      comment: "#94a3b8",
      added: "#22c55e",
      deleted: "#ef4444",
      modified: "#f59e0b",
    });
  });

  it("falls back to foreground when comment color is unavailable", () => {
    const theme = {
      colors: {
        "editor.background": "#ffffff",
        "editor.foreground": "#111827",
      },
      settings: [],
    } satisfies ThemeRegistrationRaw;

    expect(extractThemeInfo("github-light", theme).comment).toBe("#111827");
  });

  it("recognizes known light themes", () => {
    expect(isLightTheme("github-light")).toBe(true);
    expect(isLightTheme("catppuccin-latte")).toBe(true);
    expect(isLightTheme("dracula")).toBe(false);
  });
});
