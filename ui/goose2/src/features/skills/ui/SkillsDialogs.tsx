import { useTranslation } from "react-i18next";
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
import { SkillEditor } from "./SkillEditor";
import type { EditingSkill, SkillInfo } from "../api/skills";

interface SkillsDialogsProps {
  dialogOpen: boolean;
  onDialogClose: () => void;
  onSaved: (savedSkill?: SkillInfo) => void | Promise<void>;
  editingSkill?: EditingSkill;
  deletingSkill: SkillInfo | null;
  onDeletingSkillChange: (skill: SkillInfo | null) => void;
  onConfirmDelete: () => void | Promise<void>;
}

export function SkillsDialogs({
  dialogOpen,
  onDialogClose,
  onSaved,
  editingSkill,
  deletingSkill,
  onDeletingSkillChange,
  onConfirmDelete,
}: SkillsDialogsProps) {
  const { t } = useTranslation(["skills", "common"]);

  return (
    <>
      <SkillEditor
        isOpen={dialogOpen}
        onClose={onDialogClose}
        onSaved={onSaved}
        editingSkill={editingSkill}
      />

      <AlertDialog
        open={!!deletingSkill}
        onOpenChange={(open) => !open && onDeletingSkillChange(null)}
      >
        <AlertDialogContent className="max-w-sm">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("view.deleteTitle", { name: deletingSkill?.name ?? "" })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("view.deleteDescription", {
                name: deletingSkill?.name ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common:actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              className={buttonVariants({ variant: "destructive" })}
              onClick={onConfirmDelete}
            >
              {t("common:actions.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
