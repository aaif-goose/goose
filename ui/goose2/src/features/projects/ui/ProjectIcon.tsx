import { convertFileSrc } from "@tauri-apps/api/core";
import { useState, type ComponentType } from "react";
import {
  IconApi,
  IconAppWindow,
  IconBolt,
  IconBook,
  IconBrain,
  IconBrandGithub,
  IconCode,
  IconComponents,
  IconDatabase,
  IconFolder,
  IconFolderCode,
  IconGitBranch,
  IconPackage,
  IconPalette,
  IconRocket,
  IconServer,
  IconSettings,
  IconTerminal2,
  IconWorld,
} from "@tabler/icons-react";
import { cn } from "@/shared/lib/cn";

export const DEFAULT_PROJECT_ICON = "tabler:folder-code";

type TablerIconComponent = ComponentType<{
  className?: string;
  stroke?: number;
}>;

export const PROJECT_TABLER_ICONS = [
  {
    value: DEFAULT_PROJECT_ICON,
    labelKey: "dialog.iconPresets.folderCode",
    Icon: IconFolderCode,
  },
  { value: "tabler:code", labelKey: "dialog.iconPresets.code", Icon: IconCode },
  {
    value: "tabler:git-branch",
    labelKey: "dialog.iconPresets.gitBranch",
    Icon: IconGitBranch,
  },
  {
    value: "tabler:brand-github",
    labelKey: "dialog.iconPresets.github",
    Icon: IconBrandGithub,
  },
  {
    value: "tabler:terminal",
    labelKey: "dialog.iconPresets.terminal",
    Icon: IconTerminal2,
  },
  {
    value: "tabler:server",
    labelKey: "dialog.iconPresets.server",
    Icon: IconServer,
  },
  {
    value: "tabler:database",
    labelKey: "dialog.iconPresets.database",
    Icon: IconDatabase,
  },
  { value: "tabler:api", labelKey: "dialog.iconPresets.api", Icon: IconApi },
  {
    value: "tabler:app-window",
    labelKey: "dialog.iconPresets.app",
    Icon: IconAppWindow,
  },
  {
    value: "tabler:components",
    labelKey: "dialog.iconPresets.components",
    Icon: IconComponents,
  },
  {
    value: "tabler:package",
    labelKey: "dialog.iconPresets.package",
    Icon: IconPackage,
  },
  {
    value: "tabler:world",
    labelKey: "dialog.iconPresets.website",
    Icon: IconWorld,
  },
  { value: "tabler:book", labelKey: "dialog.iconPresets.docs", Icon: IconBook },
  {
    value: "tabler:palette",
    labelKey: "dialog.iconPresets.design",
    Icon: IconPalette,
  },
  { value: "tabler:brain", labelKey: "dialog.iconPresets.ai", Icon: IconBrain },
  {
    value: "tabler:bolt",
    labelKey: "dialog.iconPresets.automation",
    Icon: IconBolt,
  },
  {
    value: "tabler:rocket",
    labelKey: "dialog.iconPresets.launch",
    Icon: IconRocket,
  },
  {
    value: "tabler:settings",
    labelKey: "dialog.iconPresets.settings",
    Icon: IconSettings,
  },
] satisfies Array<{
  value: string;
  labelKey: string;
  Icon: TablerIconComponent;
}>;

const tablerIconsByValue = new Map(
  PROJECT_TABLER_ICONS.map((icon) => [icon.value, icon]),
);

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

export function ProjectIcon({
  icon,
  className,
  imageClassName,
}: {
  icon: string | null | undefined;
  className?: string;
  imageClassName?: string;
}) {
  const normalizedIcon = normalizeProjectIcon(icon);
  const [failedImageIcon, setFailedImageIcon] = useState<string | null>(null);
  const imageFailed = failedImageIcon === normalizedIcon;

  if (isImageProjectIcon(normalizedIcon) && !imageFailed) {
    const path = isFileProjectIcon(normalizedIcon)
      ? normalizedIcon.slice("file:".length)
      : normalizedIcon;
    const src =
      isFileProjectIcon(normalizedIcon) &&
      typeof window !== "undefined" &&
      window.__TAURI_INTERNALS__
        ? convertFileSrc(path)
        : path;
    return (
      <img
        src={src}
        alt=""
        className={cn("size-4 rounded-[3px] object-contain", imageClassName)}
        onError={() => setFailedImageIcon(normalizedIcon)}
      />
    );
  }

  const tablerIcon = tablerIconsByValue.get(normalizedIcon);
  const Icon = tablerIcon?.Icon ?? IconFolder;

  return <Icon className={cn("size-4", className)} stroke={1.8} />;
}
