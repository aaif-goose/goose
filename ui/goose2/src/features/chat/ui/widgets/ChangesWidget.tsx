import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { IconGitBranch } from "@tabler/icons-react";
import { cn } from "@/shared/lib/cn";
import { FileContextMenu } from "@/shared/ui/file-context-menu";
import { Skeleton } from "@/shared/ui/skeleton";
import { Spinner } from "@/shared/ui/spinner";
import type { ChangedFile } from "@/shared/types/git";
import { Widget } from "./Widget";

function splitPath(relativePath: string) {
  const lastSlash = relativePath.lastIndexOf("/");
  if (lastSlash === -1) return { dir: "", name: relativePath };
  return {
    dir: relativePath.slice(0, lastSlash + 1),
    name: relativePath.slice(lastSlash + 1),
  };
}

function ChangedFileRow({
  file,
  fullPath,
  onOpen,
}: {
  file: ChangedFile;
  fullPath: string;
  onOpen: (path: string) => void;
}) {
  const { dir, name } = splitPath(file.path);
  const isDeleted = file.status === "deleted";

  const row = (
    <button
      type="button"
      disabled={isDeleted}
      className={cn(
        "flex w-full select-none items-center gap-2 px-3 py-1.5 text-left",
        "transition-colors duration-100",
        isDeleted ? "cursor-default opacity-60" : "hover:bg-muted/80",
      )}
      onClick={isDeleted ? undefined : () => onOpen(file.path)}
    >
      <div
        className={cn(
          "flex min-w-0 flex-1 items-center overflow-hidden",
          isDeleted && "line-through",
        )}
      >
        {dir && (
          <span className="shrink truncate text-xs text-muted-foreground">
            {dir}
          </span>
        )}
        <span className="shrink-0 whitespace-nowrap text-xs font-medium text-foreground">
          {name}
        </span>
      </div>
    </button>
  );

  if (isDeleted) return row;

  return <FileContextMenu path={fullPath}>{row}</FileContextMenu>;
}

interface ChangesWidgetProps {
  files: ChangedFile[] | undefined;
  isLoading: boolean;
  currentBranch: string | null;
  repoPath: string;
  onOpenFile: (path: string) => void;
}

export function ChangesWidget({
  files,
  isLoading,
  currentBranch,
  repoPath,
  onOpenFile,
}: ChangesWidgetProps) {
  const { t } = useTranslation("chat");

  const totals = useMemo(() => {
    if (!files?.length) return { additions: 0, deletions: 0 };
    return files.reduce(
      (acc, f) => ({
        additions: acc.additions + f.additions,
        deletions: acc.deletions + f.deletions,
      }),
      { additions: 0, deletions: 0 },
    );
  }, [files]);

  const hasChanges = (files?.length ?? 0) > 0;

  const titleContent = (
    <div className="flex min-w-0 items-center gap-1">
      <span>{t("contextPanel.widgets.changes")}</span>
      {currentBranch && (
        <span className="flex min-w-0 items-center gap-1 font-normal text-muted-foreground">
          <span className="shrink-0">
            {t("contextPanel.widgets.changesOnBranch")}
          </span>
          <span className="min-w-0 truncate text-foreground">
            {currentBranch}
          </span>
        </span>
      )}
    </div>
  );

  const headerAction = (
    <div className="flex items-center gap-2">
      {isLoading && <Spinner className="size-3" />}
      {hasChanges && (
        <span className="shrink-0 font-mono text-xxs tabular-nums">
          <span className="text-text-success">+{totals.additions}</span>{" "}
          {/* i18n-check-ignore — mathematical symbol, not translatable */}
          <span className="text-text-danger">&minus;{totals.deletions}</span>
        </span>
      )}
    </div>
  );

  return (
    <Widget
      title={titleContent}
      icon={<IconGitBranch className="size-3.5 shrink-0" />}
      action={headerAction}
      flush={hasChanges}
    >
      {isLoading && !files ? (
        <div className="space-y-2 px-3 py-2.5">
          <Skeleton className="h-3 w-3/4" />
          <Skeleton className="h-3 w-1/2" />
          <Skeleton className="h-3 w-2/3" />
        </div>
      ) : hasChanges ? (
        <div className="max-h-[300px] overflow-y-auto">
          {files?.map((file) => (
            <ChangedFileRow
              key={file.path}
              file={file}
              fullPath={`${repoPath}/${file.path}`}
              onOpen={onOpenFile}
            />
          ))}
        </div>
      ) : (
        <p className="text-foreground-subtle">
          {t("contextPanel.empty.noChanges")}
        </p>
      )}
    </Widget>
  );
}
