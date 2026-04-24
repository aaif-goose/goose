import { useTranslation } from "react-i18next";
import { IconFolder, IconGitBranch, IconRefresh } from "@tabler/icons-react";
import type { CreatedWorktree, GitState } from "@/shared/types/git";
import { Button } from "@/shared/ui/button";
import { Spinner } from "@/shared/ui/spinner";
import type { ActiveWorkspace } from "../../stores/chatSessionStore";
import { Widget } from "./Widget";
import { WorkspaceActionsMenu } from "./WorkspaceActionsMenu";
import { WorkingContextPicker, shortenPath } from "./WorkingContextPicker";

interface WorkspaceWidgetProps {
  projectName?: string;
  projectColor?: string;
  projectWorkingDirs: string[];
  gitState: GitState | undefined;
  isLoading: boolean;
  isFetching: boolean;
  error: Error | null;
  activeContext: ActiveWorkspace | undefined;
  onContextChange: (context: ActiveWorkspace) => void;
  onSwitchBranch: (path: string, branch: string) => Promise<void>;
  onStashAndSwitch: (path: string, branch: string) => Promise<void>;
  onInitRepo: (path: string) => Promise<void>;
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
  onRefresh: () => void;
}

export function WorkspaceWidget({
  projectName,
  projectColor,
  projectWorkingDirs,
  gitState,
  isLoading,
  isFetching,
  error,
  activeContext,
  onContextChange,
  onSwitchBranch,
  onStashAndSwitch,
  onInitRepo,
  onFetch,
  onPull,
  onCreateBranch,
  onCreateWorktree,
  onRefresh,
}: WorkspaceWidgetProps) {
  const { t } = useTranslation("chat");
  const primaryWorkspaceRoot = projectWorkingDirs[0] ?? null;

  const gitErrorMessage =
    error instanceof Error ? error.message : t("contextPanel.errors.gitRead");

  return (
    <Widget
      title={t("contextPanel.widgets.workspace")}
      icon={<IconFolder className="size-3.5" />}
      action={
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          onClick={onRefresh}
          disabled={!primaryWorkspaceRoot || isFetching}
          className="rounded-md"
          aria-label={t("contextPanel.actions.refreshGitStatus")}
          title={t("contextPanel.actions.refreshGitStatus")}
        >
          {isFetching ? (
            <Spinner className="size-3" />
          ) : (
            <IconRefresh className="size-3" />
          )}
        </Button>
      }
    >
      <div className="space-y-2.5">
        {projectName ? (
          <div className="flex items-center gap-2">
            <span
              className="inline-block size-2 shrink-0 rounded-full"
              style={
                projectColor ? { backgroundColor: projectColor } : undefined
              }
            />
            <span className="truncate text-foreground">{projectName}</span>
          </div>
        ) : (
          <p className="text-foreground-subtle">
            {t("contextPanel.empty.noProjectAssigned")}
          </p>
        )}

        {!primaryWorkspaceRoot ? (
          <p className="truncate">{t("contextPanel.empty.folderNotSet")}</p>
        ) : isLoading && !gitState ? (
          <div className="flex items-center gap-2 text-foreground">
            <Spinner className="size-3.5" />
            <span>{t("contextPanel.states.gitLoading")}</span>
          </div>
        ) : error ? (
          <p className="text-destructive">{gitErrorMessage}</p>
        ) : gitState?.isGitRepo ? (
          <div className="space-y-2">
            <WorkingContextPicker
              currentProjectPath={primaryWorkspaceRoot}
              gitState={gitState}
              activeContext={activeContext}
              onSelect={onContextChange}
              onSwitchBranch={onSwitchBranch}
              onStashAndSwitch={onStashAndSwitch}
            />
            <WorkspaceActionsMenu
              currentProjectPath={primaryWorkspaceRoot}
              gitState={gitState}
              activeContext={activeContext}
              disabled={isFetching}
              onContextChange={onContextChange}
              onFetch={onFetch}
              onPull={onPull}
              onCreateBranch={onCreateBranch}
              onCreateWorktree={onCreateWorktree}
            />
          </div>
        ) : (
          <div className="space-y-3">
            <p className="truncate text-foreground-subtle">
              {shortenPath(primaryWorkspaceRoot)}
            </p>
            <Button
              type="button"
              variant="ghost"
              size="xs"
              onClick={() => void onInitRepo(primaryWorkspaceRoot)}
            >
              <IconGitBranch className="size-3" />
              {t("contextPanel.git.initRepo")}
            </Button>
          </div>
        )}
      </div>
    </Widget>
  );
}
