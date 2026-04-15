import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { CreatedWorktree, GitState } from "@/shared/types/git";
import { Button } from "@/shared/ui/button";
import { SplitButton } from "@/shared/ui/split-button";
import { Spinner } from "@/shared/ui/spinner";
import type { WorkingContext } from "../../stores/chatSessionStore";
import { formatErrorMessage } from "./formatError";
import {
  WorkspaceCreateDialog,
  type WorkspaceCreateMode,
} from "./WorkspaceCreateDialog";

interface WorkspaceActionsMenuProps {
  currentProjectPath: string;
  gitState: GitState;
  activeContext: WorkingContext | undefined;
  disabled?: boolean;
  onContextChange: (context: WorkingContext) => void;
  onFetch: (path: string) => Promise<void>;
  onPull: (path: string) => Promise<void>;
  onCreateBranch: (
    path: string,
    name: string,
    baseBranch: string,
  ) => Promise<void>;
  onCreateWorktree: (
    path: string,
    name: string,
    branch: string,
    createBranch: boolean,
    baseBranch?: string,
  ) => Promise<CreatedWorktree>;
}

export function WorkspaceActionsMenu({
  currentProjectPath,
  gitState,
  activeContext,
  disabled = false,
  onContextChange,
  onFetch,
  onPull,
  onCreateBranch,
  onCreateWorktree,
}: WorkspaceActionsMenuProps) {
  const { t } = useTranslation("chat");
  const [dialogMode, setDialogMode] = useState<WorkspaceCreateMode | null>(
    null,
  );
  const [activeCreateAction, setActiveCreateAction] =
    useState<WorkspaceCreateMode>("branch");
  const [runningAction, setRunningAction] = useState<"fetch" | "pull" | null>(
    null,
  );

  const defaultWorktreePath =
    gitState.worktrees.find((worktree) => worktree.path === currentProjectPath)
      ?.path ??
    gitState.worktrees[0]?.path ??
    currentProjectPath;
  const currentPath = activeContext?.path ?? defaultWorktreePath;
  const activeWorktree = useMemo(
    () =>
      gitState.worktrees.find((worktree) => worktree.path === currentPath) ??
      null,
    [currentPath, gitState.worktrees],
  );
  const activeBranch =
    activeContext?.branch ?? activeWorktree?.branch ?? gitState.currentBranch;
  const pullLabel =
    gitState.incomingCommitCount > 0
      ? t("contextPanel.git.pullWithCount", {
          count: gitState.incomingCommitCount,
        })
      : t("contextPanel.git.pull");
  const createActions = useMemo(
    () => [
      {
        id: "branch" as const,
        label: t("contextPanel.createDialog.createBranch"),
      },
      {
        id: "worktree" as const,
        label: t("contextPanel.createDialog.createWorktree"),
      },
    ],
    [t],
  );

  const runAction = async (
    action: "fetch" | "pull",
    run: () => Promise<void>,
    successKey: "fetchSuccess" | "pullSuccess",
    errorKey: "fetchError" | "pullError",
  ) => {
    setRunningAction(action);
    try {
      await run();
      toast.success(t(`contextPanel.git.${successKey}`));
    } catch (error) {
      toast.error(formatErrorMessage(error, t(`contextPanel.git.${errorKey}`)));
    } finally {
      setRunningAction(null);
    }
  };

  if (!currentPath) {
    return null;
  }

  return (
    <>
      <div className="flex w-full flex-wrap items-center gap-1.5">
        <SplitButton
          actions={createActions}
          activeActionId={activeCreateAction}
          onActionSelect={setActiveCreateAction}
          onPrimaryClick={(actionId) => {
            setActiveCreateAction(actionId);
            setDialogMode(actionId);
          }}
          disabled={disabled || runningAction !== null}
          menuTriggerLabel={t("contextPanel.actions.chooseCreateAction")}
        />
        <div className="ml-auto flex items-center gap-1.5">
          <Button
            type="button"
            variant="ghost-light"
            size="xs"
            disabled={disabled || runningAction !== null}
            onClick={() =>
              void runAction(
                "fetch",
                () => onFetch(currentPath),
                "fetchSuccess",
                "fetchError",
              )
            }
          >
            {runningAction === "fetch" ? <Spinner className="size-3" /> : null}
            {t("contextPanel.git.fetch")}
          </Button>
          <Button
            type="button"
            variant="ghost-light"
            size="xs"
            disabled={disabled || runningAction !== null}
            onClick={() =>
              void runAction(
                "pull",
                () => onPull(currentPath),
                "pullSuccess",
                "pullError",
              )
            }
          >
            {runningAction === "pull" ? <Spinner className="size-3" /> : null}
            {pullLabel}
          </Button>
        </div>
      </div>

      <WorkspaceCreateDialog
        mode={dialogMode}
        gitState={gitState}
        currentPath={currentPath}
        activeBranch={activeBranch}
        onClose={() => setDialogMode(null)}
        onContextChange={onContextChange}
        onCreateBranch={onCreateBranch}
        onCreateWorktree={onCreateWorktree}
      />
    </>
  );
}
