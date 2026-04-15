import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { FilesList } from "./FilesList";
import { useGitState } from "@/shared/hooks/useGitState";
import { useChangedFiles } from "@/shared/hooks/useChangedFiles";
import {
  createBranch,
  createWorktree,
  fetchRepo,
  initRepo,
  pullRepo,
  stashChanges,
  switchBranch,
} from "@/shared/api/git";
import type { CreatedWorktree } from "@/shared/types/git";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/shared/ui/tabs";
import { useChatSessionStore } from "../stores/chatSessionStore";
import type { WorkingContext } from "../stores/chatSessionStore";
import { WorkspaceWidget } from "./widgets/WorkspaceWidget";
import { ChangesWidget } from "./widgets/ChangesWidget";
import { ArtifactsWidget } from "./widgets/ArtifactsWidget";
import { McpServersWidget } from "./widgets/McpServersWidget";
import { openPath } from "@tauri-apps/plugin-opener";

interface ContextPanelProps {
  sessionId: string;
  projectName?: string;
  projectColor?: string;
  projectWorkingDirs?: string[];
}

type ContextPanelTab = "details" | "files";

export function ContextPanel({
  sessionId,
  projectName,
  projectColor,
  projectWorkingDirs = [],
}: ContextPanelProps) {
  const { t } = useTranslation("chat");
  const [activeTab, setActiveTab] = useState<ContextPanelTab>("details");
  const primaryWorkingDir = projectWorkingDirs[0] ?? null;

  const activeContext = useChatSessionStore(
    (s) => s.activeWorkingContextBySession[sessionId],
  );
  const setActiveWorkingContext = useChatSessionStore(
    (s) => s.setActiveWorkingContext,
  );

  const gitQueryPath = activeContext?.path ?? primaryWorkingDir;
  const {
    data: gitState,
    error,
    isLoading,
    isFetching,
    refetch,
  } = useGitState(gitQueryPath, activeTab === "details");

  const {
    data: changedFiles,
    isLoading: isFilesLoading,
    refetch: refetchFiles,
  } = useChangedFiles(gitQueryPath, activeTab === "details");

  const handleContextChange = useCallback(
    (context: WorkingContext) => {
      setActiveWorkingContext(sessionId, context);
    },
    [sessionId, setActiveWorkingContext],
  );

  const refetchAll = useCallback(async () => {
    await Promise.all([
      refetch().catch(() => undefined),
      refetchFiles().catch(() => undefined),
    ]);
  }, [refetch, refetchFiles]);

  const handleSwitchBranch = useCallback(
    async (path: string, branch: string) => {
      await switchBranch(path, branch);
      await refetchAll();
    },
    [refetchAll],
  );

  const handleStashAndSwitch = useCallback(
    async (path: string, branch: string) => {
      await stashChanges(path);
      await switchBranch(path, branch);
      await refetchAll();
    },
    [refetchAll],
  );

  const handleInitRepo = useCallback(
    async (path: string) => {
      await initRepo(path);
      await refetchAll();
    },
    [refetchAll],
  );

  const handleFetch = useCallback(
    async (path: string) => {
      await fetchRepo(path);
      await refetchAll();
    },
    [refetchAll],
  );

  const handlePull = useCallback(
    async (path: string) => {
      await pullRepo(path);
      await refetchAll();
    },
    [refetchAll],
  );

  const handleCreateBranch = useCallback(
    async (path: string, name: string, baseBranch: string) => {
      await createBranch(path, name, baseBranch);
      await refetchAll();
    },
    [refetchAll],
  );

  const handleCreateWorktree = useCallback(
    async (
      path: string,
      name: string,
      branch: string,
      createBranchForWorktree: boolean,
      baseBranch?: string,
    ): Promise<CreatedWorktree> => {
      const createdWorktree = await createWorktree(
        path,
        name,
        branch,
        createBranchForWorktree,
        baseBranch,
      );
      await refetchAll();
      return createdWorktree;
    },
    [refetchAll],
  );

  const handleOpenChangedFile = useCallback(
    (filePath: string) => {
      if (!gitQueryPath) return;
      const fullPath = `${gitQueryPath}/${filePath}`;
      void openPath(fullPath);
    },
    [gitQueryPath],
  );

  const handleRefresh = useCallback(() => {
    void refetchAll();
  }, [refetchAll]);

  useEffect(() => {
    const unlisten = listen<{ sessionId: string }>("acp:done", (event) => {
      if (event.payload.sessionId === sessionId) {
        void refetchFiles();
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [sessionId, refetchFiles]);

  return (
    <Tabs
      value={activeTab}
      onValueChange={(value) => setActiveTab(value as ContextPanelTab)}
      className="flex h-full min-w-0 flex-1 flex-col"
    >
      <div className="shrink-0 border-b border-border px-3 pb-2 pt-2.5">
        <TabsList variant="buttons">
          <TabsTrigger value="details" variant="buttons">
            {t("contextPanel.tabs.details")}
          </TabsTrigger>
          <TabsTrigger value="files" variant="buttons">
            {t("contextPanel.tabs.files")}
          </TabsTrigger>
        </TabsList>
      </div>

      <TabsContent value="details" className="flex-1 overflow-y-auto">
        <div className="space-y-2.5 px-3 pb-3 pt-2">
          <WorkspaceWidget
            projectName={projectName}
            projectColor={projectColor}
            projectWorkingDirs={projectWorkingDirs}
            gitState={gitState}
            isLoading={isLoading}
            isFetching={isFetching}
            error={error}
            activeContext={activeContext}
            onContextChange={handleContextChange}
            onSwitchBranch={handleSwitchBranch}
            onStashAndSwitch={handleStashAndSwitch}
            onInitRepo={handleInitRepo}
            onFetch={handleFetch}
            onPull={handlePull}
            onCreateBranch={handleCreateBranch}
            onCreateWorktree={handleCreateWorktree}
            onRefresh={handleRefresh}
          />
          <ChangesWidget
            files={changedFiles}
            isLoading={isFilesLoading}
            currentBranch={gitState?.currentBranch ?? null}
            repoPath={gitQueryPath ?? ""}
            onOpenFile={handleOpenChangedFile}
          />
          <ArtifactsWidget />
          <McpServersWidget />
        </div>
      </TabsContent>

      <TabsContent value="files" className="flex-1 overflow-y-auto">
        <FilesList projectWorkingDirs={projectWorkingDirs} />
      </TabsContent>
    </Tabs>
  );
}
