import * as React from "react";

type ThemePreference = "light" | "dark" | "system";
type ResolvedTheme = "light" | "dark";
type Density = "compact" | "comfortable" | "spacious";

const THEME_PREFERENCES = ["light", "dark", "system"] as const;
const DENSITIES = ["compact", "comfortable", "spacious"] as const;

type ThemeProviderProps = {
  children: React.ReactNode;
  defaultTheme?: ThemePreference;
};

type ThemeProviderState = {
  theme: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: ThemePreference) => void;
  accentColor: string;
  accentColorPreference: string;
  resetAccentColor: () => void;
  setAccentColor: (color: string) => void;
  density: Density;
  setDensity: (d: Density) => void;
};

const ThemeProviderContext = React.createContext<
  ThemeProviderState | undefined
>(undefined);

const DEFAULT_ACCENT_COLOR_PREFERENCE = "default";
const DEFAULT_LIGHT_ACCENT_COLOR = "#1a1a1a";
const DEFAULT_DARK_ACCENT_COLOR = "#ffffff";

function isDensity(value: string | null): value is Density {
  return DENSITIES.includes(value as Density);
}

function isThemePreference(value: string | null): value is ThemePreference {
  return THEME_PREFERENCES.includes(value as ThemePreference);
}

function resolveTheme(preference: ThemePreference): ResolvedTheme {
  if (preference === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return preference;
}

function getDefaultAccentColor(theme: ResolvedTheme): string {
  return theme === "dark"
    ? DEFAULT_DARK_ACCENT_COLOR
    : DEFAULT_LIGHT_ACCENT_COLOR;
}

function normalizeHexColor(color: string | null): string | null {
  const value = color?.trim();
  if (!value || value === DEFAULT_ACCENT_COLOR_PREFERENCE) return null;

  const hex = value.startsWith("#") ? value.slice(1) : value;
  if (/^[0-9a-fA-F]{3}$/.test(hex)) {
    return `#${hex
      .split("")
      .map((char) => char + char)
      .join("")
      .toLowerCase()}`;
  }
  if (/^[0-9a-fA-F]{6}$/.test(hex)) {
    return `#${hex.toLowerCase()}`;
  }

  return null;
}

function getRelativeLuminance(hexColor: string): number {
  const hex = hexColor.slice(1);
  const channels = [hex.slice(0, 2), hex.slice(2, 4), hex.slice(4, 6)].map(
    (channel) => {
      const value = Number.parseInt(channel, 16) / 255;
      return value <= 0.04045
        ? value / 12.92
        : ((value + 0.055) / 1.055) ** 2.4;
    },
  );

  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function getContrastColor(hexColor: string): string {
  const luminance = getRelativeLuminance(hexColor);
  const blackContrast = (luminance + 0.05) / 0.05;
  const whiteContrast = 1.05 / (luminance + 0.05);
  return blackContrast >= whiteContrast ? "#000000" : "#ffffff";
}

function applyAccentColor(root: HTMLElement, color: string) {
  const foreground = getContrastColor(color);
  root.style.setProperty("--brand", color);
  root.style.setProperty("--brand-foreground", foreground);
  root.style.setProperty("--color-brand", color);
  root.style.setProperty("--color-brand-foreground", foreground);
  root.style.accentColor = color;
}

function applyDensityAttribute(root: HTMLElement, density: Density) {
  if (density === "comfortable") {
    root.removeAttribute("data-density");
  } else {
    root.dataset.density = density;
  }
}

export function ThemeProvider({
  children,
  defaultTheme = "system",
}: ThemeProviderProps) {
  const [theme, setThemeState] = React.useState<ThemePreference>(() => {
    const stored = localStorage.getItem("goose-theme");
    return isThemePreference(stored) ? stored : defaultTheme;
  });

  const [resolvedTheme, setResolvedTheme] = React.useState<ResolvedTheme>(() =>
    resolveTheme(theme),
  );

  const [accentColorPreference, setAccentColorPreference] =
    React.useState<string>(() => {
      return (
        normalizeHexColor(localStorage.getItem("goose-accent-color")) ??
        DEFAULT_ACCENT_COLOR_PREFERENCE
      );
    });

  const [density, setDensityState] = React.useState<Density>(() => {
    const stored = localStorage.getItem("goose-density");
    return isDensity(stored) ? stored : "comfortable";
  });

  const accentColor = React.useMemo(() => {
    return accentColorPreference === DEFAULT_ACCENT_COLOR_PREFERENCE
      ? getDefaultAccentColor(resolvedTheme)
      : accentColorPreference;
  }, [accentColorPreference, resolvedTheme]);

  const setTheme = React.useCallback((newTheme: ThemePreference) => {
    localStorage.setItem("goose-theme", newTheme);
    setThemeState(newTheme);
  }, []);

  const setAccentColor = React.useCallback((color: string) => {
    const normalizedColor = normalizeHexColor(color);
    if (!normalizedColor) {
      localStorage.removeItem("goose-accent-color");
      setAccentColorPreference(DEFAULT_ACCENT_COLOR_PREFERENCE);
      return;
    }

    localStorage.setItem("goose-accent-color", normalizedColor);
    setAccentColorPreference(normalizedColor);
  }, []);

  const resetAccentColor = React.useCallback(() => {
    localStorage.removeItem("goose-accent-color");
    setAccentColorPreference(DEFAULT_ACCENT_COLOR_PREFERENCE);
  }, []);

  const setDensity = React.useCallback((d: Density) => {
    localStorage.setItem("goose-density", d);
    setDensityState(d);
  }, []);

  React.useEffect(() => {
    const root = window.document.documentElement;
    const resolved = resolveTheme(theme);
    setResolvedTheme(resolved);

    root.classList.remove("light", "dark");
    root.classList.add(resolved);
    root.style.colorScheme = resolved;

    if (theme === "system") {
      const mq = window.matchMedia("(prefers-color-scheme: dark)");
      const onChange = () => {
        const updated = mq.matches ? "dark" : "light";
        setResolvedTheme(updated);
        root.classList.remove("light", "dark");
        root.classList.add(updated);
        root.style.colorScheme = updated;
      };
      mq.addEventListener("change", onChange);
      return () => mq.removeEventListener("change", onChange);
    }
  }, [theme]);

  React.useEffect(() => {
    applyAccentColor(window.document.documentElement, accentColor);
  }, [accentColor]);

  React.useLayoutEffect(() => {
    applyDensityAttribute(window.document.documentElement, density);
  }, [density]);

  const value = React.useMemo(
    () => ({
      theme,
      resolvedTheme,
      setTheme,
      accentColor,
      accentColorPreference,
      resetAccentColor,
      setAccentColor,
      density,
      setDensity,
    }),
    [
      theme,
      resolvedTheme,
      setTheme,
      accentColor,
      accentColorPreference,
      resetAccentColor,
      setAccentColor,
      density,
      setDensity,
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
