import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  IconChevronDown,
  IconFolder,
  IconGitBranch,
} from "@tabler/icons-react";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { buttonVariants } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";
import type { GitState } from "@/shared/types/git";
import type { ActiveWorkspace } from "../../stores/chatSessionStore";

interface WorkingContextPickerProps {
  currentProjectPath: string | null;
  gitState: GitState | undefined;
  activeContext: ActiveWorkspace | undefined;
  onSelect: (context: ActiveWorkspace) => void;
  onSwitchBranch: (path: string, branch: string) => Promise<void>;
  onStashAndSwitch: (path: string, branch: string) => Promise<void>;
}

export function shortenPath(fullPath: string): string {
  const home =
    typeof window !== "undefined"
      ? fullPath.replace(/^\/Users\/[^/]+/, "~")
      : fullPath;
  const parts = home.split("/");
  if (parts.length > 3) {
    return `…/${parts.slice(-2).join("/")}`;
  }
  return home;
}

function worktreeName(fullPath: string): string {
  const normalizedPath = normalizeComparablePath(fullPath);
  const segments = normalizedPath.split("/");
  return segments[segments.length - 1] || fullPath;
}

function normalizeComparablePath(path: string) {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

export function WorkingContextPicker({
  currentProjectPath,
  gitState,
  activeContext,
  onSelect,
  onSwitchBranch,
  onStashAndSwitch,
}: WorkingContextPickerProps) {
  const { t } = useTranslation("chat");
  const [open, setOpen] = useState(false);
  const [pendingSwitch, setPendingSwitch] = useState<ActiveWorkspace | null>(
    null,
  );
  const [switching, setSwitching] = useState(false);

  const worktrees = gitState?.worktrees ?? [];
  const localBranches = gitState?.localBranches ?? [];
  const dirtyFileCount = gitState?.dirtyFileCount ?? 0;
  const defaultWorktreePath =
    worktrees.find(
      (worktree) =>
        normalizeComparablePath(worktree.path) ===
        normalizeComparablePath(currentProjectPath ?? ""),
    )?.path ?? worktrees[0]?.path;
  const currentPath = activeContext?.path ?? defaultWorktreePath;
  const activeWorktree =
    worktrees.find((worktree) => worktree.path === currentPath) ?? null;
  const activeBranch =
    activeContext?.branch ?? activeWorktree?.branch ?? gitState?.currentBranch;
  const activeWorktreeLabel = activeWorktree
    ? shortenPath(activeWorktree.path)
    : currentPath
      ? shortenPath(currentPath)
      : currentProjectPath
        ? shortenPath(currentProjectPath)
        : undefined;
  const activeBranchLabel = activeBranch ?? t("contextPanel.states.detached");
  const mainWorktreePath =
    gitState?.mainWorktreePath ??
    worktrees.find((worktree) => worktree.isMain)?.path ??
    null;
  const worktreeByBranch = new Map(
    worktrees
      .filter((worktree) => worktree.branch)
      .map((worktree) => [worktree.branch as string, worktree]),
  );

  const handleWorktreeSelect = useCallback(
    (path: string, branch: string | null) => {
      onSelect({ path, branch });
      setOpen(false);
    },
    [onSelect],
  );

  const finishSwitch = useCallback(
    (path: string, branch: string) => {
      onSelect({ path, branch });
      setOpen(false);
      setPendingSwitch(null);
    },
    [onSelect],
  );

  const performCarrySwitch = useCallback(
    async (path: string, branch: string) => {
      setSwitching(true);
      try {
        await onSwitchBranch(path, branch);
        finishSwitch(path, branch);
      } catch {
        toast.error(t("contextPanel.picker.switchError", { branch }));
      } finally {
        setSwitching(false);
      }
    },
    [onSwitchBranch, finishSwitch, t],
  );

  const performStashSwitch = useCallback(
    async (path: string, branch: string) => {
      setSwitching(true);
      try {
        await onStashAndSwitch(path, branch);
        finishSwitch(path, branch);
        toast.success(t("contextPanel.picker.stashSuccess", { branch }));
      } catch {
        toast.error(t("contextPanel.picker.stashError"));
      } finally {
        setSwitching(false);
      }
    },
    [onStashAndSwitch, finishSwitch, t],
  );

  const getBranchTargetPath = useCallback(
    (branch: string) => {
      const worktreeForBranch = worktreeByBranch.get(branch);
      if (worktreeForBranch) {
        return worktreeForBranch.path;
      }
      if (activeWorktree?.isMain) {
        return currentPath ?? mainWorktreePath;
      }
      return mainWorktreePath ?? currentPath;
    },
    [activeWorktree?.isMain, currentPath, mainWorktreePath, worktreeByBranch],
  );

  const handleBranchSelect = useCallback(
    (branch: string) => {
      const worktreeForBranch = worktreeByBranch.get(branch);
      if (worktreeForBranch && worktreeForBranch.path !== currentPath) {
        handleWorktreeSelect(worktreeForBranch.path, worktreeForBranch.branch);
        return;
      }
      const targetPath = getBranchTargetPath(branch);
      if (!targetPath) return;
      if (targetPath === currentPath && dirtyFileCount > 0) {
        setPendingSwitch({ path: targetPath, branch });
      } else {
        void performCarrySwitch(targetPath, branch);
      }
    },
    [
      currentPath,
      dirtyFileCount,
      getBranchTargetPath,
      handleWorktreeSelect,
      performCarrySwitch,
      worktreeByBranch,
    ],
  );

  const isWorktreeSelected = (path: string) => {
    return currentPath === path;
  };

  if (!gitState?.isGitRepo) return null;

  const hasWorktrees = worktrees.length > 0;
  const hasBranches = localBranches.length > 0;

  return (
    <>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className={cn(
              "flex w-full items-center gap-2 rounded-md border border-border px-2.5 py-2",
              "text-xs text-foreground transition-colors",
              "hover:bg-background-alt focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            )}
            aria-label={t("contextPanel.picker.selectContext")}
          >
            <IconFolder className="size-3.5 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 text-left">
              <span className="block truncate font-medium text-foreground">
                {activeWorktreeLabel ?? t("contextPanel.empty.folderNotSet")}
              </span>
              <span className="block truncate text-xxs text-foreground-subtle">
                {t("contextPanel.picker.checkedOutBranch", {
                  branch: activeBranchLabel,
                })}
              </span>
            </span>
            <IconChevronDown className="size-3 shrink-0 text-muted-foreground" />
          </button>
        </PopoverTrigger>

        <PopoverContent
          align="start"
          sideOffset={6}
          className="max-h-80 w-[var(--radix-popover-trigger-width)] min-w-56 overflow-y-auto p-1.5"
        >
          {hasWorktrees ? (
            <div>
              <p className="px-2 pb-1.5 pt-1 text-xxs font-medium uppercase tracking-wider text-muted-foreground">
                {t("contextPanel.picker.worktrees")}
              </p>
              {worktrees.map((wt) => (
                <button
                  key={wt.path}
                  type="button"
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors",
                    "hover:bg-muted focus-visible:outline-none focus-visible:bg-muted",
                    isWorktreeSelected(wt.path) && "bg-muted",
                  )}
                  onClick={() => handleWorktreeSelect(wt.path, wt.branch)}
                >
                  <IconFolder className="size-3.5 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <span className="block truncate font-medium text-foreground">
                      {worktreeName(wt.path)}
                    </span>
                    <span className="block truncate text-xxs text-foreground-subtle">
                      {t("contextPanel.picker.checkedOutBranch", {
                        branch: wt.branch ?? t("contextPanel.states.detached"),
                      })}
                    </span>
                  </div>
                </button>
              ))}
            </div>
          ) : null}

          {hasBranches ? (
            <div
              className={hasWorktrees ? "mt-1 border-t border-border pt-1" : ""}
            >
              <p className="px-2 pb-1.5 pt-1 text-xxs font-medium uppercase tracking-wider text-muted-foreground">
                {t("contextPanel.picker.allBranches")}
              </p>
              {localBranches.map((branch) => {
                const branchTargetPath = getBranchTargetPath(branch);
                const isCurrentBranch = branch === activeBranch;
                const branchMeta = isCurrentBranch
                  ? t("contextPanel.picker.currentBranch")
                  : branchTargetPath
                    ? shortenPath(branchTargetPath)
                    : null;

                return (
                  <button
                    key={branch}
                    type="button"
                    disabled={switching || isCurrentBranch}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors",
                      "hover:bg-muted focus-visible:outline-none focus-visible:bg-muted",
                      "disabled:opacity-50",
                    )}
                    onClick={() => handleBranchSelect(branch)}
                  >
                    <IconGitBranch className="size-3.5 shrink-0 text-muted-foreground" />
                    <div className="min-w-0 flex-1">
                      <span className="block truncate font-medium text-foreground">
                        {branch}
                      </span>
                      {branchMeta ? (
                        <span className="block truncate text-xxs text-muted-foreground">
                          {branchMeta}
                        </span>
                      ) : null}
                    </div>
                  </button>
                );
              })}
            </div>
          ) : null}
        </PopoverContent>
      </Popover>

      <AlertDialog
        open={pendingSwitch !== null}
        onOpenChange={(isOpen) => {
          if (!isOpen) setPendingSwitch(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("contextPanel.picker.dirtyTitle")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("contextPanel.picker.dirtyDescription", {
                count: dirtyFileCount,
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={switching}>
              {t("contextPanel.picker.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={switching}
              className={buttonVariants({ variant: "secondary" })}
              onClick={() => {
                if (pendingSwitch?.branch) {
                  void performCarrySwitch(
                    pendingSwitch.path,
                    pendingSwitch.branch,
                  );
                }
              }}
            >
              {t("contextPanel.picker.carryChanges")}
            </AlertDialogAction>
            <AlertDialogAction
              disabled={switching}
              onClick={() => {
                if (pendingSwitch?.branch) {
                  void performStashSwitch(
                    pendingSwitch.path,
                    pendingSwitch.branch,
                  );
                }
              }}
            >
              {t("contextPanel.picker.stashAndSwitch")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
