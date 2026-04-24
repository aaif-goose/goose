import * as React from "react";

type ThemePreference = "light" | "dark" | "system";
type ResolvedTheme = "light" | "dark";
type Density = "compact" | "comfortable" | "spacious";

type ThemeProviderProps = {
  children: React.ReactNode;
  defaultTheme?: ThemePreference;
};

type ThemeProviderState = {
  theme: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: ThemePreference) => void;
  accentColor: string;
  setAccentColor: (color: string) => void;
  density: Density;
  setDensity: (d: Density) => void;
};

const ThemeProviderContext = React.createContext<
  ThemeProviderState | undefined
>(undefined);

function resolveTheme(preference: ThemePreference): ResolvedTheme {
  if (preference === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return preference;
}

function getContrastColor(hexColor: string): string {
  const hex = hexColor.replace("#", "");
  const r = Number.parseInt(hex.slice(0, 2), 16);
  const g = Number.parseInt(hex.slice(2, 4), 16);
  const b = Number.parseInt(hex.slice(4, 6), 16);
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return luminance > 0.5 ? "#000000" : "#ffffff";
}

export function ThemeProvider({
  children,
  defaultTheme = "system",
}: ThemeProviderProps) {
  const [theme, setThemeState] = React.useState<ThemePreference>(() => {
    const stored = localStorage.getItem(
      "goose-theme",
    ) as ThemePreference | null;
    return stored ?? defaultTheme;
  });

  const [resolvedTheme, setResolvedTheme] = React.useState<ResolvedTheme>(() =>
    resolveTheme(theme),
  );

  const [accentColor, setAccentColorState] = React.useState<string>(() => {
    return localStorage.getItem("goose-accent-color") ?? "#3b82f6";
  });

  const [density, setDensityState] = React.useState<Density>(() => {
    const stored = localStorage.getItem("goose-density") as Density | null;
    return stored ?? "comfortable";
  });

  const setTheme = React.useCallback((newTheme: ThemePreference) => {
    localStorage.setItem("goose-theme", newTheme);
    setThemeState(newTheme);
  }, []);

  const setAccentColor = React.useCallback((color: string) => {
    localStorage.setItem("goose-accent-color", color);
    setAccentColorState(color);
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
    const root = window.document.documentElement;
    root.style.setProperty("--color-brand", accentColor);
    root.style.setProperty(
      "--color-brand-foreground",
      getContrastColor(accentColor),
    );

    const spacingScale: Record<Density, string> = {
      compact: "0.75",
      comfortable: "1",
      spacious: "1.25",
    };
    root.style.setProperty("--density-spacing", spacingScale[density]);
  }, [accentColor, density]);

  const value = React.useMemo(
    () => ({
      theme,
      resolvedTheme,
      setTheme,
      accentColor,
      setAccentColor,
      density,
      setDensity,
    }),
    [
      theme,
      resolvedTheme,
      setTheme,
      accentColor,
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
