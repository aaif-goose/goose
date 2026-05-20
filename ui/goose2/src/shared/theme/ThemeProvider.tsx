import * as React from "react";

import { createThemeVars, hexToHsl } from "./adaptive-theme";
import {
  extractThemeInfo,
  isSyntaxThemeName,
  loadThemeData,
  type SyntaxThemeName,
} from "./theme-loader";

type Density = "compact" | "comfortable" | "spacious";

type ThemeProviderProps = {
  children: React.ReactNode;
  defaultTheme?: SyntaxThemeName | "system";
};

type ThemeProviderState = {
  selectedThemeName: SyntaxThemeName | null;
  usingSystemTheme: boolean;
  resolvedThemeName: SyntaxThemeName;
  isDark: boolean;
  isLoading: boolean;
  setTheme: (themeName: SyntaxThemeName | null) => void;
  accentColor: string;
  setAccentColor: (color: string) => void;
  density: Density;
  setDensity: (density: Density) => void;
};

type CachedThemeState = {
  isDark: boolean;
  resolvedThemeName: SyntaxThemeName;
  vars: Record<string, string>;
};

type FallbackThemeInfo = {
  bg: string;
  fg: string;
  comment: string;
  added: string;
  deleted: string;
  modified: string;
};

const LEGACY_THEME_STORAGE_KEY = "goose-theme";
const THEME_MODE_STORAGE_KEY = "goose-theme-mode";
const CUSTOM_THEME_STORAGE_KEY = "goose-custom-theme";
const THEME_CACHE_STORAGE_KEY = "goose-theme-cache";
const ACCENT_STORAGE_KEY = "goose-accent-color";
const DENSITY_STORAGE_KEY = "goose-density";

const DEFAULT_ACCENT_COLOR = "#3b82f6";
const DEFAULT_SYSTEM_THEMES = {
  light: "github-light",
  dark: "github-dark",
} as const;

const BUILTIN_FALLBACK_THEMES: Record<"light" | "dark", FallbackThemeInfo> = {
  light: {
    bg: "#ffffff",
    fg: "#111827",
    comment: "#6b7280",
    added: "#1a7f37",
    deleted: "#cf222e",
    modified: "#9a6700",
  },
  dark: {
    bg: "#111827",
    fg: "#f9fafb",
    comment: "#94a3b8",
    added: "#3fb950",
    deleted: "#f85149",
    modified: "#d29922",
  },
};

export const ACCENT_COLORS = [
  { name: "blue", value: "#3b82f6" },
  { name: "cyan", value: "#06b6d4" },
  { name: "green", value: "#22c55e" },
  { name: "orange", value: "#f97316" },
  { name: "red", value: "#ef4444" },
  { name: "pink", value: "#ec4899" },
  { name: "purple", value: "#a855f7" },
  { name: "indigo", value: "#6366f1" },
] as const;

const ThemeProviderContext = React.createContext<
  ThemeProviderState | undefined
>(undefined);

function readSystemThemePreference() {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function getResolvedThemeName(
  selectedThemeName: SyntaxThemeName | null,
  systemPrefersDark: boolean,
): SyntaxThemeName {
  if (selectedThemeName) {
    return selectedThemeName;
  }

  return systemPrefersDark
    ? DEFAULT_SYSTEM_THEMES.dark
    : DEFAULT_SYSTEM_THEMES.light;
}

function getContrastColor(hexColor: string): string {
  const hex = hexColor.replace("#", "");
  const red = Number.parseInt(hex.slice(0, 2), 16);
  const green = Number.parseInt(hex.slice(2, 4), 16);
  const blue = Number.parseInt(hex.slice(4, 6), 16);
  const luminance = (0.299 * red + 0.587 * green + 0.114 * blue) / 255;
  return luminance > 0.5 ? "#000000" : "#ffffff";
}

function applyResolvedMode(isDark: boolean) {
  const root = document.documentElement;
  root.classList.remove("light", "dark");
  root.classList.add(isDark ? "dark" : "light");
  root.style.colorScheme = isDark ? "dark" : "light";
}

function applyAccentColor(hexColor: string) {
  const root = document.documentElement;
  const accentHsl = hexToHsl(hexColor);
  const foregroundHsl = hexToHsl(getContrastColor(hexColor));

  root.style.setProperty("--primary", accentHsl);
  root.style.setProperty("--primary-foreground", foregroundHsl);
  root.style.setProperty("--sidebar-primary", accentHsl);
  root.style.setProperty("--sidebar-primary-foreground", foregroundHsl);
  root.style.setProperty("--brand-color", hexColor);
  root.style.setProperty(
    "--brand-foreground-color",
    getContrastColor(hexColor),
  );
}

function clearAccentColor() {
  const root = document.documentElement;
  root.style.removeProperty("--primary");
  root.style.removeProperty("--primary-foreground");
  root.style.removeProperty("--sidebar-primary");
  root.style.removeProperty("--sidebar-primary-foreground");
  root.style.removeProperty("--brand-color");
  root.style.removeProperty("--brand-foreground-color");
}

function applyCachedTheme(): CachedThemeState | null {
  const cached = window.localStorage.getItem(THEME_CACHE_STORAGE_KEY);
  if (!cached) {
    return null;
  }

  try {
    const parsed = JSON.parse(cached) as CachedThemeState;
    const root = document.documentElement;
    for (const [key, value] of Object.entries(parsed.vars)) {
      root.style.setProperty(key, value);
    }
    applyResolvedMode(parsed.isDark);
    return parsed;
  } catch {
    return null;
  }
}

function applyDensity(density: Density) {
  const root = document.documentElement;
  if (density === "comfortable") {
    root.removeAttribute("data-density");
  } else {
    root.dataset.density = density;
  }
}

function readInitialThemeState(
  defaultTheme: SyntaxThemeName | "system",
): SyntaxThemeName | null {
  const storedThemeMode = window.localStorage.getItem(THEME_MODE_STORAGE_KEY);
  const storedCustomTheme = window.localStorage.getItem(
    CUSTOM_THEME_STORAGE_KEY,
  );
  const legacyTheme = window.localStorage.getItem(LEGACY_THEME_STORAGE_KEY);
  const hasStoredCustomTheme =
    storedCustomTheme !== null && isSyntaxThemeName(storedCustomTheme);
  const hasLegacyCustomTheme =
    legacyTheme !== null && isSyntaxThemeName(legacyTheme);

  if (hasStoredCustomTheme) {
    return storedCustomTheme;
  }

  if (
    storedThemeMode === "light" ||
    storedThemeMode === "dark" ||
    storedThemeMode === "system"
  ) {
    return null;
  }

  if (hasLegacyCustomTheme) {
    return legacyTheme;
  }

  if (
    legacyTheme === "light" ||
    legacyTheme === "dark" ||
    legacyTheme === "system"
  ) {
    return null;
  }

  return defaultTheme === "system" ? null : defaultTheme;
}

async function applyTheme(resolvedThemeName: SyntaxThemeName) {
  const themeData = await loadThemeData(resolvedThemeName);
  const info = extractThemeInfo(resolvedThemeName, themeData);
  const { isDark, vars } = createThemeVars(info.bg, info.fg, info.comment, {
    added: info.added,
    deleted: info.deleted,
    modified: info.modified,
  });

  const cachedTheme: CachedThemeState = {
    isDark,
    resolvedThemeName,
    vars,
  };

  return cachedTheme;
}

function createFallbackTheme(systemPrefersDark: boolean): CachedThemeState {
  const fallbackMode = systemPrefersDark ? "dark" : "light";
  const resolvedThemeName = DEFAULT_SYSTEM_THEMES[fallbackMode];
  const fallbackTheme = BUILTIN_FALLBACK_THEMES[fallbackMode];
  const { isDark, vars } = createThemeVars(
    fallbackTheme.bg,
    fallbackTheme.fg,
    fallbackTheme.comment,
    {
      added: fallbackTheme.added,
      deleted: fallbackTheme.deleted,
      modified: fallbackTheme.modified,
    },
  );

  return {
    isDark,
    resolvedThemeName,
    vars,
  };
}

function commitTheme(theme: CachedThemeState) {
  const root = document.documentElement;
  for (const [key, value] of Object.entries(theme.vars)) {
    root.style.setProperty(key, value);
  }
  applyResolvedMode(theme.isDark);
  window.localStorage.setItem(THEME_CACHE_STORAGE_KEY, JSON.stringify(theme));
}

export function ThemeProvider({
  children,
  defaultTheme = "system",
}: ThemeProviderProps) {
  const cachedTheme = React.useMemo(() => applyCachedTheme(), []);
  const initialSelectedTheme = React.useMemo(
    () => readInitialThemeState(defaultTheme),
    [defaultTheme],
  );
  const [selectedThemeName, setSelectedThemeName] =
    React.useState<SyntaxThemeName | null>(initialSelectedTheme);
  const [systemPrefersDark, setSystemPrefersDark] = React.useState(
    readSystemThemePreference,
  );
  const [isDark, setIsDark] = React.useState<boolean>(
    cachedTheme?.isDark ??
      document.documentElement.classList.contains("dark") ??
      false,
  );
  const [isLoading, setIsLoading] = React.useState(true);
  const [accentColor, setAccentColorState] = React.useState<string>(() => {
    return (
      window.localStorage.getItem(ACCENT_STORAGE_KEY) ?? DEFAULT_ACCENT_COLOR
    );
  });
  const [density, setDensityState] = React.useState<Density>(() => {
    const storedDensity = window.localStorage.getItem(DENSITY_STORAGE_KEY);
    return storedDensity === "compact" ||
      storedDensity === "comfortable" ||
      storedDensity === "spacious"
      ? storedDensity
      : "comfortable";
  });

  const usingSystemTheme = selectedThemeName === null;
  const resolvedThemeName = React.useMemo(
    () => getResolvedThemeName(selectedThemeName, systemPrefersDark),
    [selectedThemeName, systemPrefersDark],
  );
  const themeLoadSignature = `${selectedThemeName ?? "system"}:${resolvedThemeName}:${systemPrefersDark}`;
  const latestThemeLoadSignatureRef = React.useRef(themeLoadSignature);
  const themeLoadGenerationRef = React.useRef(0);

  if (latestThemeLoadSignatureRef.current !== themeLoadSignature) {
    latestThemeLoadSignatureRef.current = themeLoadSignature;
    themeLoadGenerationRef.current += 1;
  }
  const themeLoadGeneration = themeLoadGenerationRef.current;

  React.useEffect(() => {
    if (selectedThemeName) {
      applyAccentColor(accentColor);
      return;
    }

    clearAccentColor();
  }, [accentColor, selectedThemeName]);

  React.useEffect(() => {
    applyDensity(density);
  }, [density]);

  React.useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent) => {
      setSystemPrefersDark(event.matches);
    };

    setSystemPrefersDark(mediaQuery.matches);
    mediaQuery.addEventListener("change", onChange);
    return () => mediaQuery.removeEventListener("change", onChange);
  }, []);

  React.useEffect(() => {
    if (selectedThemeName) {
      window.localStorage.setItem(CUSTOM_THEME_STORAGE_KEY, selectedThemeName);
    } else {
      window.localStorage.removeItem(CUSTOM_THEME_STORAGE_KEY);
    }

    window.localStorage.removeItem(THEME_MODE_STORAGE_KEY);
    window.localStorage.removeItem(LEGACY_THEME_STORAGE_KEY);
  }, [selectedThemeName]);

  React.useEffect(() => {
    window.localStorage.setItem(ACCENT_STORAGE_KEY, accentColor);
  }, [accentColor]);

  React.useEffect(() => {
    window.localStorage.setItem(DENSITY_STORAGE_KEY, density);
  }, [density]);

  React.useEffect(() => {
    setIsLoading(true);

    void (async () => {
      let nextTheme: CachedThemeState;

      try {
        nextTheme = await applyTheme(resolvedThemeName);
      } catch {
        nextTheme = createFallbackTheme(systemPrefersDark);
      }

      if (themeLoadGenerationRef.current !== themeLoadGeneration) {
        return;
      }

      commitTheme(nextTheme);
      setIsDark(nextTheme.isDark);
      setIsLoading(false);
      if (selectedThemeName) {
        applyAccentColor(
          window.localStorage.getItem(ACCENT_STORAGE_KEY) ??
            DEFAULT_ACCENT_COLOR,
        );
      } else {
        clearAccentColor();
      }
    })();
  }, [
    resolvedThemeName,
    selectedThemeName,
    systemPrefersDark,
    themeLoadGeneration,
  ]);

  const setTheme = React.useCallback((themeName: SyntaxThemeName | null) => {
    setSelectedThemeName(themeName);
  }, []);

  const setAccentColor = React.useCallback((color: string) => {
    setAccentColorState(color);
  }, []);

  const setDensity = React.useCallback((nextDensity: Density) => {
    setDensityState(nextDensity);
  }, []);

  const value = React.useMemo(
    () => ({
      selectedThemeName,
      usingSystemTheme,
      resolvedThemeName,
      isDark,
      isLoading,
      setTheme,
      accentColor,
      setAccentColor,
      density,
      setDensity,
    }),
    [
      accentColor,
      density,
      isDark,
      isLoading,
      resolvedThemeName,
      setAccentColor,
      setDensity,
      setTheme,
      selectedThemeName,
      usingSystemTheme,
    ],
  );

  return (
    <ThemeProviderContext.Provider value={value}>
      {children}
    </ThemeProviderContext.Provider>
  );
}

export function useTheme() {
  const context = React.useContext(ThemeProviderContext);
  if (context === undefined) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
}
