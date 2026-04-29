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
  { value: DEFAULT_PROJECT_ICON, label: "Folder code", Icon: IconFolderCode },
  { value: "tabler:code", label: "Code", Icon: IconCode },
  { value: "tabler:git-branch", label: "Git branch", Icon: IconGitBranch },
  { value: "tabler:brand-github", label: "GitHub", Icon: IconBrandGithub },
  { value: "tabler:terminal", label: "Terminal", Icon: IconTerminal2 },
  { value: "tabler:server", label: "Server", Icon: IconServer },
  { value: "tabler:database", label: "Database", Icon: IconDatabase },
  { value: "tabler:api", label: "API", Icon: IconApi },
  { value: "tabler:app-window", label: "App", Icon: IconAppWindow },
  { value: "tabler:components", label: "Components", Icon: IconComponents },
  { value: "tabler:package", label: "Package", Icon: IconPackage },
  { value: "tabler:world", label: "Website", Icon: IconWorld },
  { value: "tabler:book", label: "Docs", Icon: IconBook },
  { value: "tabler:palette", label: "Design", Icon: IconPalette },
  { value: "tabler:brain", label: "AI", Icon: IconBrain },
  { value: "tabler:bolt", label: "Automation", Icon: IconBolt },
  { value: "tabler:rocket", label: "Launch", Icon: IconRocket },
  { value: "tabler:settings", label: "Settings", Icon: IconSettings },
] satisfies Array<{
  value: string;
  label: string;
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

export function getProjectIconLabel(icon: string): string {
  const normalizedIcon = normalizeProjectIcon(icon);
  return tablerIconsByValue.get(normalizedIcon)?.label ?? "Custom icon";
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
