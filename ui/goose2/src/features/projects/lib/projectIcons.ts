export const DEFAULT_PROJECT_ICON = "tabler:folder-code";

export const PROJECT_TABLER_ICONS = [
  {
    value: DEFAULT_PROJECT_ICON,
    labelKey: "dialog.iconPresets.folderCode",
  },
  { value: "tabler:code", labelKey: "dialog.iconPresets.code" },
  {
    value: "tabler:git-branch",
    labelKey: "dialog.iconPresets.gitBranch",
  },
  {
    value: "tabler:brand-github",
    labelKey: "dialog.iconPresets.github",
  },
  {
    value: "tabler:terminal",
    labelKey: "dialog.iconPresets.terminal",
  },
  {
    value: "tabler:server",
    labelKey: "dialog.iconPresets.server",
  },
  {
    value: "tabler:database",
    labelKey: "dialog.iconPresets.database",
  },
  { value: "tabler:api", labelKey: "dialog.iconPresets.api" },
  {
    value: "tabler:app-window",
    labelKey: "dialog.iconPresets.app",
  },
  {
    value: "tabler:components",
    labelKey: "dialog.iconPresets.components",
  },
  {
    value: "tabler:package",
    labelKey: "dialog.iconPresets.package",
  },
  {
    value: "tabler:world",
    labelKey: "dialog.iconPresets.website",
  },
  { value: "tabler:book", labelKey: "dialog.iconPresets.docs" },
  {
    value: "tabler:palette",
    labelKey: "dialog.iconPresets.design",
  },
  { value: "tabler:brain", labelKey: "dialog.iconPresets.ai" },
  {
    value: "tabler:bolt",
    labelKey: "dialog.iconPresets.automation",
  },
  {
    value: "tabler:rocket",
    labelKey: "dialog.iconPresets.launch",
  },
  {
    value: "tabler:settings",
    labelKey: "dialog.iconPresets.settings",
  },
] satisfies Array<{
  value: string;
  labelKey: string;
}>;

export function normalizeProjectIcon(icon: string | null | undefined): string {
  if (!icon || icon === "\u{1F4C1}") {
    return DEFAULT_PROJECT_ICON;
  }

  return icon;
}

export function isFileProjectIcon(icon: string): boolean {
  return icon.startsWith("file:");
}

export function isImageProjectIcon(icon: string): boolean {
  return icon.startsWith("data:image/") || isFileProjectIcon(icon);
}

export function fileProjectIconValue(path: string): string {
  return `file:${path}`;
}
