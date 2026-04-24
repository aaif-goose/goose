import { cn } from "@/shared/lib/cn";
import { Separator } from "@/shared/ui/separator";
import { ToggleGroup, ToggleGroupItem } from "@/shared/ui/toggle-group";
import { useTheme } from "@/shared/theme/ThemeProvider";
import { Sun, Moon, Monitor, Check } from "lucide-react";
import { useTranslation } from "react-i18next";

const THEME_OPTIONS = [
  { value: "light", icon: Sun },
  { value: "dark", icon: Moon },
  { value: "system", icon: Monitor },
] as const;

const ACCENT_COLORS = [
  { name: "blue", value: "#3b82f6" },
  { name: "cyan", value: "#06b6d4" },
  { name: "green", value: "#22c55e" },
  { name: "orange", value: "#f97316" },
  { name: "red", value: "#ef4444" },
  { name: "pink", value: "#ec4899" },
  { name: "purple", value: "#a855f7" },
  { name: "indigo", value: "#6366f1" },
];

const DENSITY_OPTIONS = [
  { value: "compact" },
  { value: "comfortable" },
  { value: "spacious" },
] as const;

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
        {description && (
          <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

export function AppearanceSettings() {
  const { t } = useTranslation("settings");
  const { theme, setTheme, accentColor, setAccentColor, density, setDensity } =
    useTheme();

  return (
    <div>
      <h3 className="text-lg font-semibold font-display tracking-tight">
        {t("appearance.title")}
      </h3>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("appearance.description")}
      </p>

      <Separator className="my-4" />

      <SettingRow
        label={t("appearance.theme.label")}
        description={t("appearance.theme.description")}
      >
        <ToggleGroup
          type="single"
          value={theme}
          onValueChange={(v) => v && setTheme(v as typeof theme)}
          className="gap-1 rounded-lg bg-muted p-1"
        >
          {THEME_OPTIONS.map((option) => (
            <ToggleGroupItem
              key={option.value}
              value={option.value}
              className="gap-1.5 rounded-md px-3 py-1.5 text-sm data-[state=on]:bg-background data-[state=on]:text-foreground data-[state=on]:shadow-sm"
            >
              <option.icon className="h-3.5 w-3.5" />
              {t(`appearance.theme.options.${option.value}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </SettingRow>

      <Separator className="my-4" />

      <SettingRow
        label={t("appearance.accent.label")}
        description={t("appearance.accent.description")}
      >
        <div className="grid grid-cols-4 gap-2">
          {ACCENT_COLORS.map((color) => (
            <button
              type="button"
              key={color.value}
              title={t(`appearance.accent.colors.${color.name}`)}
              onClick={() => setAccentColor(color.value)}
              className={cn(
                "flex h-7 w-7 items-center justify-center rounded-full transition-transform hover:scale-110",
                accentColor === color.value &&
                  "ring-2 ring-ring ring-offset-2 ring-offset-background",
              )}
              style={{ backgroundColor: color.value }}
            >
              {accentColor === color.value && (
                <Check className="h-3.5 w-3.5 text-white" />
              )}
            </button>
          ))}
        </div>
      </SettingRow>

      <Separator className="my-4" />

      <SettingRow
        label={t("appearance.density.label")}
        description={t("appearance.density.description")}
      >
        <ToggleGroup
          type="single"
          value={density}
          onValueChange={(v) => v && setDensity(v as typeof density)}
          className="gap-1 rounded-lg bg-muted p-1"
        >
          {DENSITY_OPTIONS.map((option) => (
            <ToggleGroupItem
              key={option.value}
              value={option.value}
              className="rounded-md px-3 py-1.5 text-sm data-[state=on]:bg-background data-[state=on]:text-foreground data-[state=on]:shadow-sm"
            >
              {t(`appearance.density.options.${option.value}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </SettingRow>
    </div>
  );
}
