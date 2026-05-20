import { describe, expect, it } from "vitest";

import { createThemeVars, hexToHsl, luminance } from "./adaptive-theme";

describe("adaptive-theme", () => {
  it("converts hex colors to HSL component strings", () => {
    expect(hexToHsl("#ffffff")).toBe("0 0% 100.0%");
    expect(hexToHsl("#000000")).toBe("0 0% 0.0%");
    expect(hexToHsl("#3b82f6")).toBe("217.2 91.22% 59.8%");
  });

  it("detects luminance correctly for light and dark colors", () => {
    expect(luminance("#ffffff")).toBeGreaterThan(luminance("#111827"));
    expect(luminance("#111827")).toBeLessThan(0.5);
    expect(luminance("#f8fafc")).toBeGreaterThan(0.5);
  });

  it("derives dark theme vars from dark syntax colors", () => {
    const result = createThemeVars("#111827", "#f9fafb", "#94a3b8", {
      added: "#22c55e",
      deleted: "#ef4444",
      modified: "#f59e0b",
    });

    expect(result.isDark).toBe(true);
    expect(result.vars["--background"]).toBe(hexToHsl("#111827"));
    expect(result.vars["--foreground"]).toBe(hexToHsl("#f9fafb"));
    expect(result.vars["--muted-foreground"]).toBe(hexToHsl("#94a3b8"));
    expect(result.vars["--destructive-foreground"]).toBe(hexToHsl("#ffffff"));
    expect(result.vars["--status-added"]).toBe("#22c55e");
    expect(result.vars["--status-deleted"]).toBe("#ef4444");
    expect(result.vars["--status-modified"]).toBe("#f59e0b");
    expect(result.vars["--ui-warning-bg"]).toBe("rgba(245, 158, 11, 0.1)");
    expect(result.vars["--sidebar-background"]).toMatch(
      /\d+\.\d+ \d+\.\d+% \d+\.\d+%/,
    );
  });

  it("derives light theme vars from light syntax colors and light-mode warning alpha", () => {
    const result = createThemeVars("#f8fafc", "#0f172a", "#64748b");

    expect(result.isDark).toBe(false);
    expect(result.vars["--foreground"]).toBe(hexToHsl("#0f172a"));
    expect(result.vars["--muted-foreground"]).toBe(hexToHsl("#64748b"));
    expect(result.vars["--destructive-foreground"]).toBe(hexToHsl("#ffffff"));
    expect(result.vars["--status-added"]).toBe("#1a7f37");
    expect(result.vars["--status-deleted"]).toBe("#cf222e");
    expect(result.vars["--status-modified"]).toBe("#9a6700");
    expect(result.vars["--ui-warning-bg"]).toBe("rgba(154, 103, 0, 0.08)");
  });
});
