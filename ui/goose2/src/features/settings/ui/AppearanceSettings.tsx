import { useMemo, useRef, useState } from "react";
import { Moon, Sun, Search, Check, MonitorSmartphone } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import { Separator } from "@/shared/ui/separator";
import { ToggleGroup, ToggleGroupItem } from "@/shared/ui/toggle-group";
import { ACCENT_COLORS, useTheme } from "@/shared/theme/ThemeProvider";
import {
  isLightTheme,
  SYNTAX_THEMES,
  type SyntaxThemeName,
} from "@/shared/theme/theme-loader";
import { useTranslation } from "react-i18next";

const DENSITY_OPTIONS = [
  { value: "compact" },
  { value: "comfortable" },
  { value: "spacious" },
] as const;

function formatThemeLabel(name: string) {
  return name
    .split("-")
    .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
    .join(" ");
}

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-8 py-3">
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">{label}</p>
        {description ? (
          <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
        ) : null}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

export function AppearanceSettings() {
  const { t } = useTranslation("settings");
  const {
    selectedThemeName,
    usingSystemTheme,
    setTheme,
    accentColor,
    setAccentColor,
    density,
    setDensity,
  } = useTheme();
  const [search, setSearch] = useState("");
  const didScrollRef = useRef(false);

  const filteredThemes = useMemo(() => {
    const query = search.toLowerCase().trim();
    if (!query) {
      return SYNTAX_THEMES;
    }

    return SYNTAX_THEMES.filter((themeName) => themeName.includes(query));
  }, [search]);

  const activeThemeRef = (node: HTMLButtonElement | null) => {
    if (!node || didScrollRef.current) {
      return;
    }

    didScrollRef.current = true;
    node.scrollIntoView({ block: "center" });
  };

  return (
    <div>
      <h3 className="font-display text-lg font-semibold tracking-tight">
        {t("appearance.title")}
      </h3>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("appearance.description")}
      </p>

      <Separator className="my-4" />

      <div className="space-y-3">
        <div>
          <p className="text-sm font-medium">{t("appearance.theme.label")}</p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t("appearance.theme.description")}
          </p>
        </div>

        <button
          aria-pressed={usingSystemTheme}
          className={cn(
            "flex w-full items-center gap-3 rounded-lg border border-border/70 bg-background/70 px-3 py-2 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
            usingSystemTheme
              ? "border-primary/30 bg-primary/10 text-foreground"
              : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
          )}
          data-testid="theme-option-system"
          onClick={() => {
            setTheme(null);
          }}
          type="button"
        >
          <MonitorSmartphone className="h-4 w-4 shrink-0" />
          <div className="min-w-0 flex-1">
            <div className="truncate font-medium">
              {t("appearance.theme.systemLabel")}
            </div>
            <div className="truncate text-xs text-muted-foreground">
              {t("appearance.theme.systemDescription")}
            </div>
          </div>
          {usingSystemTheme ? (
            <Check className="h-4 w-4 shrink-0 text-primary" />
          ) : null}
        </button>

        <div className="relative">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <input
            className="w-full rounded-lg border border-border/70 bg-background/70 py-2 pl-9 pr-3 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            data-testid="theme-search-input"
            onChange={(event) => {
              didScrollRef.current = false;
              setSearch(event.target.value);
            }}
            placeholder={t("appearance.theme.searchPlaceholder")}
            type="text"
            value={search}
          />
        </div>

        <div className="max-h-72 overflow-y-auto rounded-lg border border-border/70 bg-background/70">
          {filteredThemes.length === 0 ? (
            <p className="px-3 py-4 text-center text-sm text-muted-foreground">
              {t("appearance.theme.empty")}
            </p>
          ) : (
            filteredThemes.map((themeName) => {
              const selected = selectedThemeName === themeName;
              const ThemeIcon = isLightTheme(themeName) ? Sun : Moon;

              return (
                <button
                  aria-pressed={selected}
                  className={cn(
                    "flex w-full items-center gap-3 px-3 py-2 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
                    selected
                      ? "bg-primary/10 text-foreground"
                      : "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
                  )}
                  data-testid={`theme-option-${themeName}`}
                  key={themeName}
                  onClick={() => {
                    setTheme(themeName as SyntaxThemeName);
                  }}
                  ref={selected ? activeThemeRef : undefined}
                  type="button"
                >
                  <ThemeIcon className="h-4 w-4 shrink-0" />
                  <span className="flex-1 truncate">
                    {formatThemeLabel(themeName)}
                  </span>
                  {selected ? (
                    <Check className="h-4 w-4 shrink-0 text-primary" />
                  ) : null}
                </button>
              );
            })
          )}
        </div>
      </div>

      <Separator className="my-4" />

      <SettingRow
        description={t(
          usingSystemTheme
            ? "appearance.accent.disabledDescription"
            : "appearance.accent.description",
        )}
        label={t("appearance.accent.label")}
      >
        <div className="grid grid-cols-4 gap-2">
          {ACCENT_COLORS.map((color) => (
            <button
              className={cn(
                "flex h-7 w-7 items-center justify-center rounded-full transition-transform hover:scale-110 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:scale-100",
                accentColor === color.value &&
                  "ring-2 ring-ring ring-offset-2 ring-offset-background",
              )}
              data-testid={`accent-color-${color.name}`}
              disabled={usingSystemTheme}
              key={color.value}
              onClick={() => setAccentColor(color.value)}
              style={{ backgroundColor: color.value }}
              title={t(`appearance.accent.colors.${color.name}`)}
              type="button"
            >
              {accentColor === color.value ? (
                <Check className="h-3.5 w-3.5 text-white" />
              ) : null}
            </button>
          ))}
        </div>
      </SettingRow>

      <Separator className="my-4" />

      <SettingRow
        description={t("appearance.density.description")}
        label={t("appearance.density.label")}
      >
        <ToggleGroup
          className="gap-1 rounded-lg bg-muted p-1"
          onValueChange={(value) => {
            if (value) {
              setDensity(value as typeof density);
            }
          }}
          type="single"
          value={density}
        >
          {DENSITY_OPTIONS.map((option) => (
            <ToggleGroupItem
              className="rounded-md px-3 py-1.5 text-sm data-[state=on]:bg-background data-[state=on]:text-foreground data-[state=on]:shadow-sm"
              key={option.value}
              value={option.value}
            >
              {t(`appearance.density.options.${option.value}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </SettingRow>
    </div>
  );
}
