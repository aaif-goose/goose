import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { normalizeDialogSelection } from "./useChatInputAttachments";

interface UseChatInputFilePickerOptions {
  disabled: boolean;
  addPathAttachments: (paths: string[]) => Promise<void>;
}

export function useChatInputFilePicker({
  disabled,
  addPathAttachments,
}: UseChatInputFilePickerOptions) {
  const { t } = useTranslation("chat");

  const handleAttachFiles = useCallback(async () => {
    if (disabled) {
      return;
    }

    try {
      const selected = await open({
        title: t("attachments.chooseFilesDialogTitle"),
        multiple: true,
      });
      await addPathAttachments(normalizeDialogSelection(selected));
    } catch {
      // Dialog plugin may be unavailable in some environments.
    }
  }, [addPathAttachments, disabled, t]);

  const handleAttachFolders = useCallback(async () => {
    if (disabled) {
      return;
    }

    try {
      const selected = await open({
        directory: true,
        title: t("attachments.chooseFoldersDialogTitle"),
        multiple: true,
      });
      await addPathAttachments(normalizeDialogSelection(selected));
    } catch {
      // Dialog plugin may be unavailable in some environments.
    }
  }, [addPathAttachments, disabled, t]);

  return { handleAttachFiles, handleAttachFolders };
}
