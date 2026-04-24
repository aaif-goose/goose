import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { CreatedWorktree, GitState } from "@/shared/types/git";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";
import type { ActiveWorkspace } from "../../stores/chatSessionStore";
import { formatErrorMessage } from "./formatError";
import { shortenPath } from "./WorkingContextPicker";

const UNSET_SELECT_VALUE = "__unset__";

export type WorkspaceCreateMode = "branch" | "worktree";

interface WorkspaceCreateDialogProps {
  mode: WorkspaceCreateMode | null;
  gitState: GitState;
  currentPath: string;
  activeBranch: string | null;
  onClose: () => void;
  onContextChange: (context: ActiveWorkspace) => void;
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

function worktreePreviewPath(rootPath: string, name: string) {
  const normalizedPath = rootPath.replace(/\/+$/, "");
  const pathSegments = normalizedPath.split("/");
  const repoName = pathSegments[pathSegments.length - 1];
  const parentPath = pathSegments.slice(0, -1).join("/");
  return `${parentPath}/${repoName}-worktrees/${name}`;
}

export function WorkspaceCreateDialog({
  mode,
  gitState,
  currentPath,
  activeBranch,
  onClose,
  onContextChange,
  onCreateBranch,
  onCreateWorktree,
}: WorkspaceCreateDialogProps) {
  const { t } = useTranslation("chat");
  const [branchName, setBranchName] = useState("");
  const [baseBranch, setBaseBranch] = useState("");
  const [worktreeName, setWorktreeName] = useState("");
  const [useNewBranch, setUseNewBranch] = useState(true);
  const [existingBranch, setExistingBranch] = useState("");
  const [branchNameManuallyEdited, setBranchNameManuallyEdited] =
    useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const occupiedBranches = useMemo(
    () =>
      new Set(
        gitState.worktrees
          .map((worktree) => worktree.branch)
          .filter((branch): branch is string => Boolean(branch)),
      ),
    [gitState.worktrees],
  );
  const availableExistingBranches = useMemo(
    () =>
      gitState.localBranches.filter((branch) => !occupiedBranches.has(branch)),
    [gitState.localBranches, occupiedBranches],
  );
  const defaultBaseBranch =
    activeBranch ?? gitState.currentBranch ?? gitState.localBranches[0] ?? "";
  const previewRootPath = gitState.mainWorktreePath ?? currentPath;
  const trimmedBranchName = branchName.trim();
  const trimmedWorktreeName = worktreeName.trim();
  const previewPath =
    mode === "worktree" && trimmedWorktreeName
      ? shortenPath(worktreePreviewPath(previewRootPath, trimmedWorktreeName))
      : null;
  const branchDialogValid =
    trimmedBranchName.length > 0 && baseBranch.length > 0 && !saving;
  const worktreeDialogValid =
    trimmedWorktreeName.length > 0 &&
    (useNewBranch
      ? trimmedBranchName.length > 0 && baseBranch.length > 0
      : existingBranch.length > 0) &&
    !saving;
  const canSubmit =
    mode === "branch"
      ? branchDialogValid
      : mode === "worktree"
        ? worktreeDialogValid
        : false;

  useEffect(() => {
    if (!mode) {
      return;
    }
    const defaultUseNewBranch = true;
    setBranchName("");
    setBaseBranch(defaultBaseBranch);
    setWorktreeName("");
    setUseNewBranch(defaultUseNewBranch);
    setExistingBranch(
      defaultUseNewBranch ? "" : (availableExistingBranches[0] ?? ""),
    );
    setBranchNameManuallyEdited(false);
    setError(null);
    setSaving(false);
  }, [availableExistingBranches, defaultBaseBranch, mode]);

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!mode) {
      return;
    }

    setSaving(true);
    setError(null);

    try {
      if (mode === "branch") {
        await onCreateBranch(currentPath, trimmedBranchName, baseBranch);
        onContextChange({ path: currentPath, branch: trimmedBranchName });
        toast.success(
          t("contextPanel.createDialog.branchSuccess", {
            branch: trimmedBranchName,
          }),
        );
        onClose();
        return;
      }

      const targetBranch = useNewBranch ? trimmedBranchName : existingBranch;
      const createdWorktree = await onCreateWorktree(
        currentPath,
        trimmedWorktreeName,
        targetBranch,
        useNewBranch,
        useNewBranch ? baseBranch : undefined,
      );
      onContextChange({
        path: createdWorktree.path,
        branch: createdWorktree.branch,
      });
      toast.success(
        t("contextPanel.createDialog.worktreeSuccess", {
          worktree: trimmedWorktreeName,
        }),
      );
      onClose();
    } catch (submitError) {
      const fallback =
        mode === "branch"
          ? t("contextPanel.createDialog.branchError", {
              branch: trimmedBranchName,
            })
          : t("contextPanel.createDialog.worktreeError", {
              worktree: trimmedWorktreeName,
            });
      const message = formatErrorMessage(submitError, fallback);
      setError(message);
      toast.error(message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={mode !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-md gap-0 p-0">
        <DialogHeader className="px-5 py-4">
          <DialogTitle className="text-sm">
            {mode === "branch"
              ? t("contextPanel.createDialog.branchTitle")
              : t("contextPanel.createDialog.worktreeTitle")}
          </DialogTitle>
          <DialogDescription className="text-xs">
            {mode === "branch"
              ? t("contextPanel.createDialog.branchDescription")
              : t("contextPanel.createDialog.worktreeDescription")}
          </DialogDescription>
        </DialogHeader>

        <form
          id="workspace-create-form"
          onSubmit={handleSubmit}
          className="space-y-4 px-5 pb-5"
        >
          {mode === "branch" ? (
            <>
              <div className="space-y-1.5">
                <Label
                  htmlFor="workspace-branch-name"
                  className="text-xs font-medium text-muted-foreground"
                >
                  {t("contextPanel.createDialog.branchName")}
                </Label>
                <Input
                  id="workspace-branch-name"
                  value={branchName}
                  onChange={(event) => {
                    setBranchName(event.target.value);
                    setError(null);
                  }}
                  placeholder={t(
                    "contextPanel.createDialog.branchNamePlaceholder",
                  )}
                />
              </div>

              <div className="space-y-1.5">
                <Label className="text-xs font-medium text-muted-foreground">
                  {t("contextPanel.createDialog.baseBranch")}
                </Label>
                <Select value={baseBranch} onValueChange={setBaseBranch}>
                  <SelectTrigger className="w-full">
                    <SelectValue
                      placeholder={t("contextPanel.createDialog.baseBranch")}
                    />
                  </SelectTrigger>
                  <SelectContent>
                    {gitState.localBranches.map((branch) => (
                      <SelectItem key={branch} value={branch}>
                        {branch}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </>
          ) : null}

          {mode === "worktree" ? (
            <>
              <div className="space-y-1.5">
                <Label
                  htmlFor="workspace-worktree-name"
                  className="text-xs font-medium text-muted-foreground"
                >
                  {t("contextPanel.createDialog.worktreeName")}
                </Label>
                <Input
                  id="workspace-worktree-name"
                  value={worktreeName}
                  onChange={(event) => {
                    const nextWorktreeName = event.target.value;
                    setWorktreeName(nextWorktreeName);
                    if (useNewBranch && !branchNameManuallyEdited) {
                      setBranchName(nextWorktreeName);
                    }
                    setError(null);
                  }}
                  placeholder={t(
                    "contextPanel.createDialog.worktreeNamePlaceholder",
                  )}
                />
                {previewPath ? (
                  <p className="text-xxs text-muted-foreground">
                    {t("contextPanel.createDialog.worktreePath", {
                      path: previewPath,
                    })}
                  </p>
                ) : null}
              </div>

              {availableExistingBranches.length > 0 ? (
                <div className="flex items-center gap-2">
                  <Checkbox
                    id="workspace-create-new-branch"
                    checked={useNewBranch}
                    onCheckedChange={(checked) => {
                      const nextUseNewBranch = checked === true;
                      setUseNewBranch(nextUseNewBranch);
                      if (nextUseNewBranch && !branchNameManuallyEdited) {
                        setBranchName(worktreeName);
                      }
                      setError(null);
                    }}
                  />
                  <Label
                    htmlFor="workspace-create-new-branch"
                    className="cursor-pointer text-xs font-medium text-muted-foreground"
                  >
                    {t("contextPanel.createDialog.createNewBranch")}
                  </Label>
                </div>
              ) : null}

              {useNewBranch ? (
                <>
                  <div className="space-y-1.5">
                    <Label
                      htmlFor="workspace-worktree-branch-name"
                      className="text-xs font-medium text-muted-foreground"
                    >
                      {t("contextPanel.createDialog.branchName")}
                    </Label>
                    <Input
                      id="workspace-worktree-branch-name"
                      value={branchName}
                      onChange={(event) => {
                        setBranchNameManuallyEdited(true);
                        setBranchName(event.target.value);
                        setError(null);
                      }}
                      placeholder={t(
                        "contextPanel.createDialog.branchNamePlaceholder",
                      )}
                    />
                  </div>

                  <div className="space-y-1.5">
                    <Label className="text-xs font-medium text-muted-foreground">
                      {t("contextPanel.createDialog.baseBranch")}
                    </Label>
                    <Select value={baseBranch} onValueChange={setBaseBranch}>
                      <SelectTrigger className="w-full">
                        <SelectValue
                          placeholder={t(
                            "contextPanel.createDialog.baseBranch",
                          )}
                        />
                      </SelectTrigger>
                      <SelectContent>
                        {gitState.localBranches.map((branch) => (
                          <SelectItem key={branch} value={branch}>
                            {branch}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                </>
              ) : (
                <div className="space-y-1.5">
                  <Label className="text-xs font-medium text-muted-foreground">
                    {t("contextPanel.createDialog.branchToOpen")}
                  </Label>
                  <Select
                    value={existingBranch || UNSET_SELECT_VALUE}
                    onValueChange={(value) =>
                      setExistingBranch(
                        value === UNSET_SELECT_VALUE ? "" : value,
                      )
                    }
                  >
                    <SelectTrigger className="w-full">
                      <SelectValue
                        placeholder={t(
                          "contextPanel.createDialog.branchToOpen",
                        )}
                      />
                    </SelectTrigger>
                    <SelectContent>
                      {availableExistingBranches.length > 0 ? (
                        availableExistingBranches.map((branch) => (
                          <SelectItem key={branch} value={branch}>
                            {branch}
                          </SelectItem>
                        ))
                      ) : (
                        <SelectItem disabled value={UNSET_SELECT_VALUE}>
                          {t("contextPanel.createDialog.noAvailableBranches")}
                        </SelectItem>
                      )}
                    </SelectContent>
                  </Select>
                </div>
              )}
            </>
          ) : null}

          {error ? <p className="text-xs text-destructive">{error}</p> : null}
        </form>

        <DialogFooter className="border-t px-5 py-4">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onClose}
            disabled={saving}
          >
            {t("contextPanel.createDialog.cancel")}
          </Button>
          <Button
            type="submit"
            form="workspace-create-form"
            size="sm"
            disabled={!canSubmit}
          >
            {saving
              ? mode === "branch"
                ? t("contextPanel.createDialog.creatingBranch")
                : t("contextPanel.createDialog.creatingWorktree")
              : mode === "branch"
                ? t("contextPanel.createDialog.createBranch")
                : t("contextPanel.createDialog.createWorktree")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
