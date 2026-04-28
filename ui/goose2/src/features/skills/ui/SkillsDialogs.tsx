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
import { CreateSkillDialog } from "./CreateSkillDialog";
import type { SkillInfo } from "../api/skills";

interface SkillsDialogsProps {
  dialogOpen: boolean;
  onDialogClose: () => void;
  onCreated: () => void | Promise<void>;
  editingSkill?: {
    name: string;
    description: string;
    instructions: string;
    path: string;
    fileLocation: string;
  };
  deletingSkill: SkillInfo | null;
  onDeletingSkillChange: (skill: SkillInfo | null) => void;
  onConfirmDelete: () => void | Promise<void>;
  notification: string | null;
}

export function SkillsDialogs({
  dialogOpen,
  onDialogClose,
  onCreated,
  editingSkill,
  deletingSkill,
  onDeletingSkillChange,
  onConfirmDelete,
  notification,
}: SkillsDialogsProps) {
  const { t } = useTranslation(["skills", "common"]);

  return (
    <>
      <CreateSkillDialog
        isOpen={dialogOpen}
        onClose={onDialogClose}
        onCreated={onCreated}
        editingSkill={editingSkill}
      />

      <AlertDialog
        open={!!deletingSkill}
        onOpenChange={(open) => !open && onDeletingSkillChange(null)}
      >
        <AlertDialogContent className="max-w-sm">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("view.deleteTitle")}</AlertDialogTitle>
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

      {notification && (
        <div className="fixed bottom-4 right-4 z-50 rounded-lg border border-border bg-background px-4 py-3 text-sm shadow-popover animate-in fade-in slide-in-from-bottom-2">
          {notification}
        </div>
      )}
    </>
  );
}
