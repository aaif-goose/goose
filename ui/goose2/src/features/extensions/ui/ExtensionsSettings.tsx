import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { IconPlus, IconSearch } from "@tabler/icons-react";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import {
  listExtensions,
  addExtension,
  removeExtension,
  toggleExtension,
  nameToKey,
} from "../api/extensions";
import {
  getDisplayName,
  type ExtensionConfig,
  type ExtensionEntry,
} from "../types";
import { ExtensionItem } from "./ExtensionItem";
import { ExtensionModal } from "./ExtensionModal";

export function ExtensionsSettings() {
  const { t } = useTranslation("settings");
  const [extensions, setExtensions] = useState<ExtensionEntry[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [modalMode, setModalMode] = useState<"add" | "edit" | null>(null);
  const [editingExtension, setEditingExtension] =
    useState<ExtensionEntry | null>(null);
  const [searchTerm, setSearchTerm] = useState("");

  const fetchExtensions = useCallback(async () => {
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

  const matchesSearch = useCallback(
    (ext: ExtensionEntry) => {
      if (!searchTerm) return true;
      const q = searchTerm.toLowerCase();
      return (
        getDisplayName(ext).toLowerCase().includes(q) ||
        ext.name.toLowerCase().includes(q) ||
        (ext.description ?? "").toLowerCase().includes(q) ||
        ext.type.toLowerCase().includes(q)
      );
    },
    [searchTerm],
  );

  const sorted = useMemo(() => {
    return [...extensions].sort((a, b) => {
      if (a.type === "builtin" && b.type !== "builtin") return -1;
      if (a.type !== "builtin" && b.type === "builtin") return 1;
      const aBundled = a.bundled === true;
      const bBundled = b.bundled === true;
      if (aBundled && !bBundled) return -1;
      if (!aBundled && bBundled) return 1;
      return a.name.localeCompare(b.name);
    });
  }, [extensions]);

  const enabled = useMemo(
    () => sorted.filter((e) => e.enabled && matchesSearch(e)),
    [sorted, matchesSearch],
  );
  const available = useMemo(
    () => sorted.filter((e) => !e.enabled && matchesSearch(e)),
    [sorted, matchesSearch],
  );

  const handleToggle = async (ext: ExtensionEntry) => {
    try {
      await toggleExtension(ext.config_key, !ext.enabled);
      await fetchExtensions();
    } catch {
      toast.error(t("extensions.errors.toggleFailed"));
    }
  };

  const handleConfigure = (ext: ExtensionEntry) => {
    setEditingExtension(ext);
    setModalMode("edit");
  };

  const handleSubmit = async (
    name: string,
    config: ExtensionConfig,
    extensionEnabled: boolean,
  ) => {
    try {
      const newKey = nameToKey(name);
      const isEdit = !!editingExtension;
      const isAdd = !editingExtension;
      const keyChanged = isEdit && editingExtension.config_key !== newKey;

      if (
        (isAdd || keyChanged) &&
        extensions.some((e) => e.config_key === newKey)
      ) {
        toast.error(t("extensions.errors.nameConflict", { name }));
        return;
      }

      await addExtension(name, config, extensionEnabled);
      if (keyChanged) {
        await removeExtension(editingExtension.config_key);
      }
      setModalMode(null);
      setEditingExtension(null);
      await fetchExtensions();
    } catch {
      toast.error(t("extensions.errors.saveFailed"));
    }
  };

  const handleDelete = async (configKey: string) => {
    try {
      await removeExtension(configKey);
      setModalMode(null);
      setEditingExtension(null);
      await fetchExtensions();
    } catch {
      toast.error(t("extensions.errors.deleteFailed"));
    }
  };

  const handleModalClose = () => {
    setModalMode(null);
    setEditingExtension(null);
  };

  return (
    <div className="space-y-6">
      <div>
        <h3 className="font-display text-lg font-semibold tracking-tight">
          {t("extensions.title")}
        </h3>
        <p className="mt-1 text-sm text-muted-foreground">
          {t("extensions.description")}
        </p>
      </div>

      <div className="relative">
        <IconSearch className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          placeholder={t("extensions.search")}
          className="pl-9"
        />
      </div>

      {isLoading ? (
        <div className="divide-y divide-border">
          {[1, 2, 3].map((i) => (
            <div key={i} className="h-14 animate-pulse bg-muted/30" />
          ))}
        </div>
      ) : extensions.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("extensions.empty")}</p>
      ) : enabled.length === 0 && available.length === 0 && searchTerm ? (
        <p className="text-sm text-muted-foreground">
          {t("extensions.noResults")}
        </p>
      ) : (
        <div className="space-y-4">
          {enabled.length > 0 && (
            <div>
              <h4 className="text-sm font-medium">
                {t("extensions.enabledCount", { count: enabled.length })}
              </h4>
              <div className="divide-y divide-border">
                {enabled.map((ext) => (
                  <ExtensionItem
                    key={ext.config_key}
                    extension={ext}
                    onToggle={handleToggle}
                    onConfigure={handleConfigure}
                  />
                ))}
              </div>
            </div>
          )}

          {available.length > 0 && (
            <div>
              <h4 className="text-sm font-medium text-muted-foreground">
                {t("extensions.availableCount", { count: available.length })}
              </h4>
              <div className="divide-y divide-border">
                {available.map((ext) => (
                  <ExtensionItem
                    key={ext.config_key}
                    extension={ext}
                    onToggle={handleToggle}
                    onConfigure={handleConfigure}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => {
          setEditingExtension(null);
          setModalMode("add");
        }}
      >
        <IconPlus className="size-4" />
        {t("extensions.addExtension")}
      </Button>

      {modalMode === "add" && (
        <ExtensionModal onSubmit={handleSubmit} onClose={handleModalClose} />
      )}

      {modalMode === "edit" && editingExtension && (
        <ExtensionModal
          extension={editingExtension}
          onSubmit={handleSubmit}
          onDelete={handleDelete}
          onClose={handleModalClose}
        />
      )}
    </div>
  );
}
