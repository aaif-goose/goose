import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { useFileImportZone } from "@/shared/hooks/useFileImportZone";
import { createZipArchive } from "../api/archive";
import {
  exportSkill,
  importSkills,
  importSkillsArchive,
  type SkillInfo,
} from "../api/skills";
import { formatSkillError } from "../lib/formatSkillError";
import { downloadExport } from "../lib/skillsHelpers";

export function useSkillImportExport(onAfterImport: () => Promise<void>) {
  const { t } = useTranslation(["skills", "common"]);

  const handleExport = async (skill: SkillInfo) => {
    if (skill.sourceKind === "builtin") {
      return;
    }

    try {
      const result = await exportSkill(skill.path);
      downloadExport(result.data, result.filename, result.mimeType);
      toast.success(t("view.exportedTo", { filename: result.filename }));
    } catch {
      toast.error(t("view.exportError"));
    }
  };

  const handleImport = async (fileBytes: number[], fileName: string) => {
    await importSkills(fileBytes, fileName);
    await onAfterImport();
    toast.success(t("view.importSuccess"));
  };

  const handleImportDirectory = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: true,
        title: t("common:actions.import"),
      });

      if (!selected || Array.isArray(selected)) {
        return;
      }

      const archive = await createZipArchive(selected);
      await importSkillsArchive(archive.data, archive.filename);
      await onAfterImport();
      toast.success(t("view.importSuccess"));
    } catch (error) {
      toast.error(formatSkillError(error, t("view.importError")));
    }
  };

  const fileImport = useFileImportZone({
    onImportFile: (fileBytes, fileName) => {
      void handleImport(fileBytes, fileName).catch((error) => {
        toast.error(formatSkillError(error, t("view.importError")));
      });
    },
  });

  return { ...fileImport, handleImportDirectory, handleExport };
}
