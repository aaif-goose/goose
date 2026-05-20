import type { ThemeRegistrationRaw } from "shiki";

export const SYNTAX_THEMES = [
  "andromeeda",
  "aurora-x",
  "ayu-dark",
  "catppuccin-frappe",
  "catppuccin-latte",
  "catppuccin-macchiato",
  "catppuccin-mocha",
  "dark-plus",
  "dracula",
  "dracula-soft",
  "everforest-dark",
  "everforest-light",
  "github-dark",
  "github-dark-default",
  "github-dark-dimmed",
  "github-dark-high-contrast",
  "github-light",
  "github-light-default",
  "github-light-high-contrast",
  "gruvbox-dark-hard",
  "gruvbox-dark-medium",
  "gruvbox-dark-soft",
  "gruvbox-light-hard",
  "gruvbox-light-medium",
  "gruvbox-light-soft",
  "houston",
  "kanagawa-dragon",
  "kanagawa-lotus",
  "kanagawa-wave",
  "laserwave",
  "light-plus",
  "material-theme",
  "material-theme-darker",
  "material-theme-lighter",
  "material-theme-ocean",
  "material-theme-palenight",
  "min-dark",
  "min-light",
  "monokai",
  "night-owl",
  "nord",
  "one-dark-pro",
  "one-light",
  "plastic",
  "poimandres",
  "red",
  "rose-pine",
  "rose-pine-dawn",
  "rose-pine-moon",
  "slack-dark",
  "slack-ochin",
  "snazzy-light",
  "solarized-dark",
  "solarized-light",
  "synthwave-84",
  "tokyo-night",
  "vesper",
  "vitesse-black",
  "vitesse-dark",
  "vitesse-light",
] as const;

export type SyntaxThemeName = (typeof SYNTAX_THEMES)[number];

export const LIGHT_THEMES: ReadonlySet<SyntaxThemeName> = new Set([
  "catppuccin-latte",
  "everforest-light",
  "github-light",
  "github-light-default",
  "github-light-high-contrast",
  "gruvbox-light-hard",
  "gruvbox-light-medium",
  "gruvbox-light-soft",
  "kanagawa-lotus",
  "light-plus",
  "material-theme-lighter",
  "min-light",
  "one-light",
  "rose-pine-dawn",
  "slack-ochin",
  "snazzy-light",
  "solarized-light",
  "vitesse-light",
]);

const themeImports: Record<
  SyntaxThemeName,
  () => Promise<{ default: ThemeRegistrationRaw }>
> = {
  andromeeda: () => import("shiki/themes/andromeeda.mjs"),
  "aurora-x": () => import("shiki/themes/aurora-x.mjs"),
  "ayu-dark": () => import("shiki/themes/ayu-dark.mjs"),
  "catppuccin-frappe": () => import("shiki/themes/catppuccin-frappe.mjs"),
  "catppuccin-latte": () => import("shiki/themes/catppuccin-latte.mjs"),
  "catppuccin-macchiato": () => import("shiki/themes/catppuccin-macchiato.mjs"),
  "catppuccin-mocha": () => import("shiki/themes/catppuccin-mocha.mjs"),
  "dark-plus": () => import("shiki/themes/dark-plus.mjs"),
  dracula: () => import("shiki/themes/dracula.mjs"),
  "dracula-soft": () => import("shiki/themes/dracula-soft.mjs"),
  "everforest-dark": () => import("shiki/themes/everforest-dark.mjs"),
  "everforest-light": () => import("shiki/themes/everforest-light.mjs"),
  "github-dark": () => import("shiki/themes/github-dark.mjs"),
  "github-dark-default": () => import("shiki/themes/github-dark-default.mjs"),
  "github-dark-dimmed": () => import("shiki/themes/github-dark-dimmed.mjs"),
  "github-dark-high-contrast": () =>
    import("shiki/themes/github-dark-high-contrast.mjs"),
  "github-light": () => import("shiki/themes/github-light.mjs"),
  "github-light-default": () => import("shiki/themes/github-light-default.mjs"),
  "github-light-high-contrast": () =>
    import("shiki/themes/github-light-high-contrast.mjs"),
  "gruvbox-dark-hard": () => import("shiki/themes/gruvbox-dark-hard.mjs"),
  "gruvbox-dark-medium": () => import("shiki/themes/gruvbox-dark-medium.mjs"),
  "gruvbox-dark-soft": () => import("shiki/themes/gruvbox-dark-soft.mjs"),
  "gruvbox-light-hard": () => import("shiki/themes/gruvbox-light-hard.mjs"),
  "gruvbox-light-medium": () => import("shiki/themes/gruvbox-light-medium.mjs"),
  "gruvbox-light-soft": () => import("shiki/themes/gruvbox-light-soft.mjs"),
  houston: () => import("shiki/themes/houston.mjs"),
  "kanagawa-dragon": () => import("shiki/themes/kanagawa-dragon.mjs"),
  "kanagawa-lotus": () => import("shiki/themes/kanagawa-lotus.mjs"),
  "kanagawa-wave": () => import("shiki/themes/kanagawa-wave.mjs"),
  laserwave: () => import("shiki/themes/laserwave.mjs"),
  "light-plus": () => import("shiki/themes/light-plus.mjs"),
  "material-theme": () => import("shiki/themes/material-theme.mjs"),
  "material-theme-darker": () =>
    import("shiki/themes/material-theme-darker.mjs"),
  "material-theme-lighter": () =>
    import("shiki/themes/material-theme-lighter.mjs"),
  "material-theme-ocean": () => import("shiki/themes/material-theme-ocean.mjs"),
  "material-theme-palenight": () =>
    import("shiki/themes/material-theme-palenight.mjs"),
  "min-dark": () => import("shiki/themes/min-dark.mjs"),
  "min-light": () => import("shiki/themes/min-light.mjs"),
  monokai: () => import("shiki/themes/monokai.mjs"),
  "night-owl": () => import("shiki/themes/night-owl.mjs"),
  nord: () => import("shiki/themes/nord.mjs"),
  "one-dark-pro": () => import("shiki/themes/one-dark-pro.mjs"),
  "one-light": () => import("shiki/themes/one-light.mjs"),
  plastic: () => import("shiki/themes/plastic.mjs"),
  poimandres: () => import("shiki/themes/poimandres.mjs"),
  red: () => import("shiki/themes/red.mjs"),
  "rose-pine": () => import("shiki/themes/rose-pine.mjs"),
  "rose-pine-dawn": () => import("shiki/themes/rose-pine-dawn.mjs"),
  "rose-pine-moon": () => import("shiki/themes/rose-pine-moon.mjs"),
  "slack-dark": () => import("shiki/themes/slack-dark.mjs"),
  "slack-ochin": () => import("shiki/themes/slack-ochin.mjs"),
  "snazzy-light": () => import("shiki/themes/snazzy-light.mjs"),
  "solarized-dark": () => import("shiki/themes/solarized-dark.mjs"),
  "solarized-light": () => import("shiki/themes/solarized-light.mjs"),
  "synthwave-84": () => import("shiki/themes/synthwave-84.mjs"),
  "tokyo-night": () => import("shiki/themes/tokyo-night.mjs"),
  vesper: () => import("shiki/themes/vesper.mjs"),
  "vitesse-black": () => import("shiki/themes/vitesse-black.mjs"),
  "vitesse-dark": () => import("shiki/themes/vitesse-dark.mjs"),
  "vitesse-light": () => import("shiki/themes/vitesse-light.mjs"),
};

interface ThemeSetting {
  scope?: string | string[];
  settings?: { foreground?: string };
}

export interface ThemeInfo {
  name: string;
  bg: string;
  fg: string;
  comment: string;
  added: string | null;
  deleted: string | null;
  modified: string | null;
}

function extractCommentColor(
  settings: ReadonlyArray<ThemeSetting> | undefined,
  fallback: string,
): string {
  if (!settings) {
    return fallback;
  }

  for (const setting of settings) {
    if (!setting.scope || !setting.settings?.foreground) {
      continue;
    }

    const scopes = Array.isArray(setting.scope)
      ? setting.scope
      : [setting.scope];
    if (scopes.includes("comment")) {
      return setting.settings.foreground;
    }
  }

  return fallback;
}

function stripAlpha(color: string): string {
  if (color.length === 9 && color.startsWith("#")) {
    return color.slice(0, 7);
  }
  return color;
}

function extractGitColors(colors: Record<string, string> | undefined) {
  if (!colors) {
    return { added: null, deleted: null, modified: null };
  }

  const addedKeys = [
    "gitDecoration.addedResourceForeground",
    "editorGutter.addedBackground",
    "diffEditor.insertedTextBackground",
  ];
  const deletedKeys = [
    "gitDecoration.deletedResourceForeground",
    "editorGutter.deletedBackground",
    "diffEditor.removedTextBackground",
  ];
  const modifiedKeys = [
    "gitDecoration.modifiedResourceForeground",
    "editorGutter.modifiedBackground",
  ];

  const findColor = (keys: string[]) => {
    for (const key of keys) {
      const value = colors[key];
      if (value) {
        return stripAlpha(value);
      }
    }
    return null;
  };

  return {
    added: findColor(addedKeys),
    deleted: findColor(deletedKeys),
    modified: findColor(modifiedKeys),
  };
}

export function isLightTheme(name: string): boolean {
  return LIGHT_THEMES.has(name as SyntaxThemeName);
}

export function isSyntaxThemeName(name: string): name is SyntaxThemeName {
  return (SYNTAX_THEMES as readonly string[]).includes(name);
}

export function extractThemeInfo(
  themeName: string,
  theme: ThemeRegistrationRaw,
): ThemeInfo {
  const background =
    (theme.colors?.["editor.background"] as string | undefined) ?? "#1e1e1e";
  const foreground =
    (theme.colors?.["editor.foreground"] as string | undefined) ?? "#d4d4d4";
  const gitColors = extractGitColors(
    theme.colors as Record<string, string> | undefined,
  );

  return {
    name: themeName,
    bg: background,
    fg: foreground,
    comment: extractCommentColor(
      theme.settings as ReadonlyArray<ThemeSetting> | undefined,
      foreground,
    ),
    ...gitColors,
  };
}

export async function loadThemeData(
  name: SyntaxThemeName,
): Promise<ThemeRegistrationRaw> {
  const loader = themeImports[name];
  const { default: theme } = await loader();
  return theme;
}
