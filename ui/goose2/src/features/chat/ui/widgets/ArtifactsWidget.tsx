import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  IconFile,
  IconFileCode,
  IconFileText,
  IconJson,
  IconMarkdown,
  IconPhoto,
  IconFileDescription,
} from "@tabler/icons-react";
import { FileContextMenu } from "@/shared/ui/file-context-menu";
import {
  useArtifactPolicyContext,
  type SessionArtifact,
} from "../../hooks/ArtifactPolicyContext";
import { Widget } from "./Widget";

const CODE_EXTENSIONS = new Set([
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".mjs",
  ".cjs",
  ".css",
  ".scss",
  ".html",
  ".py",
  ".rb",
  ".rs",
  ".go",
  ".java",
  ".sh",
  ".sql",
  ".yaml",
  ".yml",
]);

const IMAGE_EXTENSIONS = new Set([
  ".png",
  ".jpg",
  ".jpeg",
  ".gif",
  ".webp",
  ".svg",
]);

function getArtifactIcon(artifact: SessionArtifact) {
  const ext = artifact.filename.includes(".")
    ? `.${artifact.filename.split(".").pop()?.toLowerCase()}`
    : "";

  if (ext === ".json") return IconJson;
  if (ext === ".md" || ext === ".mdx") return IconMarkdown;
  if (ext === ".txt") return IconFileText;
  if (IMAGE_EXTENSIONS.has(ext)) return IconPhoto;
  if (CODE_EXTENSIONS.has(ext)) return IconFileCode;
  return IconFile;
}

export function ArtifactsWidget() {
  const { t } = useTranslation("chat");
  const { getAllSessionArtifacts, openResolvedPath } =
    useArtifactPolicyContext();

  const artifacts = useMemo(
    () => getAllSessionArtifacts(),
    [getAllSessionArtifacts],
  );

  if (artifacts.length === 0) {
    return null;
  }

  return (
    <Widget
      title={t("contextPanel.widgets.artifacts")}
      icon={<IconFileDescription className="size-3.5" />}
      action={
        <span className="text-xxs text-foreground-subtle">
          {artifacts.length}
        </span>
      }
      flush
    >
      {artifacts.map((artifact) => {
        const Icon = getArtifactIcon(artifact);
        return (
          <FileContextMenu
            key={artifact.resolvedPath}
            path={artifact.resolvedPath}
          >
            <button
              type="button"
              className="flex w-full select-none items-center gap-2 px-3 py-1.5 text-left transition-colors duration-100 hover:bg-muted/80"
              onClick={() => void openResolvedPath(artifact.resolvedPath)}
            >
              <Icon className="size-3.5 shrink-0 text-foreground-subtle" />
              <span className="truncate text-xs text-foreground">
                {artifact.filename}
              </span>
            </button>
          </FileContextMenu>
        );
      })}
    </Widget>
  );
}
