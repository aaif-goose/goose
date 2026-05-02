import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  addExtension,
  listExtensions,
  removeExtension,
} from "../api/extensions";
import { nameToKey } from "../lib/extensionKeys";
import type { ExtensionConfig, ExtensionEntry } from "../types";

type ExtensionModalMode = "add" | "edit" | null;

export function useExtensionsSettings() {
  const { t } = useTranslation("settings");
  const [extensions, setExtensions] = useState<ExtensionEntry[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [modalMode, setModalMode] = useState<ExtensionModalMode>(null);
  const [editingExtension, setEditingExtension] =
    useState<ExtensionEntry | null>(null);

  const fetchExtensions = useCallback(async () => {
    setIsLoading(true);
    try {
      const result = await listExtensions();
      setExtensions(result);
    } catch {
      setExtensions([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void fetchExtensions();
  }, [fetchExtensions]);

  const handleAdd = useCallback(() => {
    setEditingExtension(null);
    setModalMode("add");
  }, []);

  const handleConfigure = useCallback((extension: ExtensionEntry) => {
    setEditingExtension(extension);
    setModalMode("edit");
  }, []);

  const handleSubmit = useCallback(
    async (name: string, config: ExtensionConfig) => {
      try {
        const newKey = nameToKey(name);
        const isEdit = !!editingExtension;
        const isAdd = !editingExtension;
        const keyChanged = isEdit && editingExtension.config_key !== newKey;

        if (
          (isAdd || keyChanged) &&
          extensions.some((extension) => extension.config_key === newKey)
        ) {
          toast.error(t("extensions.errors.nameConflict", { name }));
          return;
        }

        await addExtension(name, config, editingExtension?.enabled ?? false);
        if (keyChanged) {
          await removeExtension(editingExtension.config_key);
        }
        setModalMode(null);
        setEditingExtension(null);
        await fetchExtensions();
      } catch {
        await fetchExtensions();
        toast.error(t("extensions.errors.saveFailed"));
      }
    },
    [editingExtension, extensions, fetchExtensions, t],
  );

  const handleDelete = useCallback(
    async (configKey: string) => {
      try {
        await removeExtension(configKey);
        setModalMode(null);
        setEditingExtension(null);
        await fetchExtensions();
      } catch (error) {
        toast.error(t("extensions.errors.deleteFailed"));
        throw error;
      }
    },
    [fetchExtensions, t],
  );

  const handleModalClose = useCallback(() => {
    setModalMode(null);
    setEditingExtension(null);
  }, []);

  return {
    extensions,
    isLoading,
    modalMode,
    editingExtension,
    handleAdd,
    handleConfigure,
    handleSubmit,
    handleDelete,
    handleModalClose,
  };
}
