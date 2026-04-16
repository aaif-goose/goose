import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import { Textarea } from "@/shared/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { createSkill, updateSkill } from "../api/skills";

const KEBAB_CASE_REGEX = /^[a-z0-9]+(-[a-z0-9]+)*$/;

interface CreateSkillDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onCreated?: () => void;
  editingSkill?: { name: string; description: string; instructions: string };
}

export function CreateSkillDialog({
  isOpen,
  onClose,
  onCreated,
  editingSkill,
}: CreateSkillDialogProps) {
  const { t } = useTranslation(["skills", "common"]);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [instructions, setInstructions] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isEditing = !!editingSkill;

  // Pre-fill fields when editing
  useEffect(() => {
    if (isOpen && editingSkill) {
      setName(editingSkill.name);
      setDescription(editingSkill.description);
      setInstructions(editingSkill.instructions);
      setError(null);
    } else if (isOpen) {
      setName("");
      setDescription("");
      setInstructions("");
      setError(null);
    }
  }, [isOpen, editingSkill]);

  const nameValid = name.length > 0 && KEBAB_CASE_REGEX.test(name);
  const canSave = nameValid && description.trim().length > 0 && !saving;

  const handleNameChange = (raw: string) => {
    if (isEditing) return; // name is read-only in edit mode
    const formatted = raw
      .toLowerCase()
      .replace(/[^a-z0-9-]/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-/, "");
    setName(formatted);
    setError(null);
  };

  const handleClose = () => {
    setName("");
    setDescription("");
    setInstructions("");
    setError(null);
    onClose();
  };

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!canSave) return;
    setSaving(true);
    setError(null);
    try {
      if (isEditing) {
        await updateSkill(name, description.trim(), instructions);
      } else {
        await createSkill(name, description.trim(), instructions);
      }
      setName("");
      setDescription("");
      setInstructions("");
      onCreated?.();
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
          id="skill-form"
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
              onChange={(e) => handleNameChange(e.target.value)}
              placeholder={t("dialog.namePlaceholder")}
              readOnly={isEditing}
              className={cn(isEditing && "opacity-60 cursor-not-allowed")}
            />
            {name.length > 0 && !nameValid && (
              <p className="text-xs text-destructive">
                {t("dialog.nameValidation")}
              </p>
            )}
          </div>

          {/* Description */}
          <div className="space-y-1">
            <Label className="text-xs font-medium text-muted-foreground">
              {t("dialog.description")}{" "}
              <span className="text-destructive">*</span>
            </Label>
            <Input
              value={description}
              onChange={(e) => {
                setDescription(e.target.value);
                setError(null);
              }}
              placeholder={t("dialog.descriptionPlaceholder")}
            />
          </div>

          {/* Instructions */}
          <div className="space-y-1">
            <Label className="text-xs font-medium text-muted-foreground">
              {t("dialog.instructions")}
            </Label>
            <Textarea
              value={instructions}
              onChange={(e) => setInstructions(e.target.value)}
              rows={10}
              placeholder={t("dialog.instructionsPlaceholder")}
              className="text-xs font-mono leading-relaxed"
            />
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
          <Button type="submit" form="skill-form" size="sm" disabled={!canSave}>
            {saving
              ? isEditing
                ? t("dialog.saving")
                : t("dialog.creating")
              : isEditing
                ? t("common:actions.saveChanges")
                : t("dialog.createSkill")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
