import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
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
import {
  createSkill,
  updateSkill,
  type EditingSkill,
  type SkillInfo,
} from "../api/skills";
import { formatSkillName, isValidSkillName } from "../lib/skillsHelpers";
import { getRenamedSkillFileLocation } from "../lib/skillsPath";

interface SkillEditorProps {
  isOpen: boolean;
  onClose: () => void;
  onSaved?: (savedSkill?: SkillInfo) => void | Promise<void>;
  editingSkill?: EditingSkill;
}

export function SkillEditor({
  isOpen,
  onClose,
  onSaved,
  editingSkill,
}: SkillEditorProps) {
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

  const nameValid = isValidSkillName(name);
  const canSave = nameValid && description.trim().length > 0 && !saving;

  const handleNameChange = (raw: string) => {
    setName(formatSkillName(raw));
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
      let savedSkill: SkillInfo | undefined;
      if (isEditing) {
        savedSkill = await updateSkill(
          editingSkill.path,
          name,
          description.trim(),
          instructions,
        );
      } else {
        await createSkill(name, description.trim(), instructions);
      }
      setName("");
      setDescription("");
      setInstructions("");
      await onSaved?.(savedSkill);
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

          {isEditing && editingSkill ? (
            <p className="-mt-2 break-all text-[11px] text-muted-foreground">
              {t("dialog.pathOnDisk")}:{" "}
              {getRenamedSkillFileLocation(editingSkill.fileLocation, name)}
            </p>
          ) : null}

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
