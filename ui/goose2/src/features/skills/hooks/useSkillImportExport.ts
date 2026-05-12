import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useFileImportZone } from "@/shared/hooks/useFileImportZone";
import { exportSkill, importSkills, type SkillInfo } from "../api/skills";
import { downloadExport } from "../lib/skillsHelpers";

export function useSkillImportExport(onAfterImport: () => Promise<void>) {
  const { t } = useTranslation(["skills"]);

  const handleExport = async (skill: SkillInfo) => {
    if (skill.sourceKind === "builtin") {
      return;
    }

    try {
      const result = await exportSkill(skill.path);
      downloadExport(result.json, result.filename);
      toast.success(t("view.exportedTo", { filename: result.filename }));
    } catch {
      toast.error(t("view.exportError"));
    }
  };

  const handleImport = async (fileBytes: number[], fileName: string) => {
    try {
      await importSkills(fileBytes, fileName);
      await onAfterImport();
      toast.success(t("view.importSuccess"));
    } catch {
      toast.error(t("view.importError"));
    }
  };

  const fileImport = useFileImportZone({ onImportFile: handleImport });

  return { ...fileImport, handleExport };
}
