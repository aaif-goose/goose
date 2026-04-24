import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { FolderOpen } from "lucide-react";
import { cn } from "@/shared/lib/cn";
import { getHomeDir } from "@/shared/api/system";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";
import {
  createProject,
  updateProject,
  type ProjectInfo,
} from "../api/projects";
import { discoverAcpProviders, type AcpProvider } from "@/shared/api/acp";
import {
  buildEditorText,
  hasEquivalentWorkingDir,
  insertWorkingDir,
  parseEditorText,
} from "../lib/projectPromptText";
import { PromptEditor } from "./PromptEditor";

const COLOR_OPTIONS = [
  "#64748b",
  "#ef4444",
  "#f97316",
  "#f59e0b",
  "#22c55e",
  "#10b981",
  "#14b8a6",
  "#06b6d4",
  "#3b82f6",
  "#6366f1",
  "#8b5cf6",
  "#a855f7",
  "#ec4899",
  "#f43f5e",
];

function getDefaultProjectName(path: string | null | undefined): string {
  const trimmed = path?.trim();
  if (!trimmed) {
    return "";
  }

  const normalized = trimmed.replace(/[\\/]+$/, "");
  const parts = normalized.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? "";
}

interface CreateProjectDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onCreated: (project: ProjectInfo) => void;
  initialWorkingDir?: string | null;
  editingProject?: {
    id: string;
    name: string;
    description: string;
    prompt: string;
    icon: string;
    color: string;
    preferredProvider: string | null;
    preferredModel: string | null;
    workingDirs: string[];
    useWorktrees: boolean;
  };
}

export function CreateProjectDialog({
  isOpen,
  onClose,
  onCreated,
  initialWorkingDir,
  editingProject,
}: CreateProjectDialogProps) {
  const { t } = useTranslation(["projects", "common"]);
  const [name, setName] = useState("");
  const [prompt, setPrompt] = useState("");
  const icon = "\u{1F4C1}";
  const [color, setColor] = useState(COLOR_OPTIONS[0]);
  const [preferredProvider, setPreferredProvider] = useState<string | null>(
    null,
  );
  const preferredModel: string | null = null;
  const [useWorktrees, setUseWorktrees] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [acpProviders, setAcpProviders] = useState<AcpProvider[]>([]);

  const isEditing = !!editingProject;

  useEffect(() => {
    discoverAcpProviders()
      .then(setAcpProviders)
      .catch(() => setAcpProviders([]));
  }, []);

  const handleAddDirectory = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("dialog.addDirectoryDialogTitle"),
      });
      if (selected && typeof selected === "string") {
        const homeDir = await getHomeDir().catch(() => null);

        setPrompt((prev) => {
          if (hasEquivalentWorkingDir(prev, selected, homeDir)) {
            return prev;
          }

          return insertWorkingDir(prev, selected);
        });
      }
    } catch {
      // Dialog plugin not available
    }
  };

  // Pre-fill fields when the dialog opens or when the project identity changes,
  // but NOT on every parent re-render (which would reset user edits mid-typing).
  const prevOpenRef = useRef(false);
  const prevEditingIdRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    const justOpened = isOpen && !prevOpenRef.current;
    prevOpenRef.current = isOpen;

    const projectIdChanged =
      isOpen && !justOpened && editingProject?.id !== prevEditingIdRef.current;
    prevEditingIdRef.current = editingProject?.id;

    if (!justOpened && !projectIdChanged) return;

    if (editingProject) {
      setName(editingProject.name);
      setPrompt(
        buildEditorText(editingProject.workingDirs, editingProject.prompt),
      );
      setColor(editingProject.color);
      setPreferredProvider(editingProject.preferredProvider ?? null);
      setUseWorktrees(editingProject.useWorktrees);
      setError(null);
    } else {
      setName(getDefaultProjectName(initialWorkingDir));
      setPrompt(
        buildEditorText(
          initialWorkingDir?.trim() ? [initialWorkingDir.trim()] : [],
          "",
        ),
      );
      setColor(COLOR_OPTIONS[0]);
      setPreferredProvider(null);
      setUseWorktrees(false);
      setError(null);
    }
  }, [isOpen, editingProject, initialWorkingDir]);

  const canSave = name.trim().length > 0 && !saving;

  const handleClose = () => {
    setName("");
    setPrompt("");
    setColor(COLOR_OPTIONS[0]);
    setPreferredProvider(null);
    setUseWorktrees(false);
    setError(null);
    onClose();
  };

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSave) return;
    setSaving(true);
    setError(null);
    const { prompt: parsedPrompt, workingDirs } = parseEditorText(prompt);
    try {
      let savedProject: ProjectInfo;
      if (isEditing) {
        savedProject = await updateProject(
          editingProject.id,
          name.trim(),
          "",
          parsedPrompt,
          icon,
          color,
          preferredProvider || null,
          preferredModel,
          workingDirs,
          useWorktrees,
        );
      } else {
        savedProject = await createProject(
          name.trim(),
          "",
          parsedPrompt,
          icon,
          color,
          preferredProvider || null,
          preferredModel,
          workingDirs,
          useWorktrees,
        );
      }
      onCreated(savedProject);
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && handleClose()}>
      <DialogContent className="max-w-lg max-h-[85vh] flex flex-col gap-0 p-0">
        <DialogHeader className="shrink-0 px-5 py-4">
          <DialogTitle className="text-sm">
            {isEditing ? t("dialog.editTitle") : t("dialog.newTitle")}
          </DialogTitle>
        </DialogHeader>

        <form
          id="project-form"
          onSubmit={handleSave}
          className="min-h-0 flex-1 overflow-y-auto space-y-4 px-5 pb-5"
        >
          {/* Name */}
          <div className="space-y-1">
            <Label className="text-xs font-medium text-muted-foreground">
              {t("dialog.name")} <span className="text-destructive">*</span>
            </Label>
            <Input
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                setError(null);
              }}
              placeholder={t("dialog.namePlaceholder")}
            />
          </div>

          {/* Instructions */}
          <div className="space-y-1">
            <Label className="text-xs font-medium text-muted-foreground">
              {t("dialog.instructions")}
            </Label>
            <PromptEditor
              value={prompt}
              onChange={setPrompt}
              ariaLabel={t("dialog.instructions")}
              placeholder={t("dialog.instructionsPlaceholder")}
            />
            <Button
              type="button"
              variant="outline"
              size="xs"
              onClick={handleAddDirectory}
              className="mt-1.5"
            >
              <FolderOpen className="size-3.5" />
              {t("dialog.addDirectory")}
            </Button>
          </div>

          {/* Color */}
          <div className="space-y-1">
            <Label className="text-xs font-medium text-muted-foreground">
              {t("dialog.color")}
            </Label>
            <div className="flex flex-wrap gap-1.5 pt-1">
              {COLOR_OPTIONS.map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => setColor(c)}
                  className={cn(
                    "h-6 w-6 rounded-full border-2 transition-transform",
                    color === c
                      ? "border-foreground scale-110"
                      : "border-transparent hover:scale-105",
                  )}
                  style={{ backgroundColor: c }}
                  aria-label={t("dialog.colorAria", { color: c })}
                />
              ))}
            </div>
          </div>

          {/* Provider */}
          <div className="space-y-1.5">
            <Label className="text-xs font-medium text-muted-foreground">
              {t("dialog.provider")}
            </Label>
            <Select
              value={preferredProvider ?? "__none__"}
              onValueChange={(v) =>
                setPreferredProvider(v === "__none__" ? null : v)
              }
            >
              <SelectTrigger className="w-full">
                <SelectValue placeholder={t("dialog.noneUseDefault")} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">
                  {t("dialog.noneUseDefault")}
                </SelectItem>
                {acpProviders.map((p) => (
                  <SelectItem key={p.id} value={p.id}>
                    {p.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {/* Use Worktrees */}
          <div className="flex items-center gap-2">
            <Checkbox
              id="use-worktrees"
              checked={useWorktrees}
              onCheckedChange={(checked) => setUseWorktrees(checked === true)}
            />
            <Label
              htmlFor="use-worktrees"
              className="text-xs font-medium text-muted-foreground cursor-pointer"
            >
              {t("dialog.useWorktrees")}
            </Label>
          </div>

          {/* Error */}
          {error && <p className="text-xs text-destructive">{error}</p>}
        </form>

        <DialogFooter className="shrink-0 border-t px-5 py-4">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={handleClose}
            disabled={saving}
          >
            {t("common:actions.cancel")}
          </Button>
          <Button
            type="submit"
            form="project-form"
            size="sm"
            disabled={!canSave}
          >
            {saving
              ? isEditing
                ? t("dialog.saving")
                : t("dialog.creating")
              : isEditing
                ? t("common:actions.saveChanges")
                : t("dialog.createProject")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
