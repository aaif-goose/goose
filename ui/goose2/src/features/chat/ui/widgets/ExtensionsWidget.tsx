import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconPuzzle, IconSearch } from "@tabler/icons-react";
import { Input } from "@/shared/ui/input";
import { Widget } from "./Widget";
import { listExtensions } from "@/features/extensions/api/extensions";
import {
  getDisplayName,
  type ExtensionEntry,
} from "@/features/extensions/types";

export function ExtensionsWidget() {
  const { t } = useTranslation("chat");
  const [extensions, setExtensions] = useState<ExtensionEntry[]>([]);
  const [searchTerm, setSearchTerm] = useState("");

  const fetchEnabled = useCallback(() => {
    listExtensions()
      .then((all) => setExtensions(all.filter((e) => e.enabled)))
      .catch(() => setExtensions([]));
  }, []);

  useEffect(() => {
    fetchEnabled();
    const handleVisibility = () => {
      if (document.visibilityState === "visible") fetchEnabled();
    };
    document.addEventListener("visibilitychange", handleVisibility);
    window.addEventListener("focus", fetchEnabled);
    return () => {
      document.removeEventListener("visibilitychange", handleVisibility);
      window.removeEventListener("focus", fetchEnabled);
    };
  }, [fetchEnabled]);

  const filtered = useMemo(() => {
    if (!searchTerm) return extensions;
    const q = searchTerm.toLowerCase();
    return extensions.filter((ext) => {
      const name = getDisplayName(ext).toLowerCase();
      return (
        name.includes(q) || (ext.description ?? "").toLowerCase().includes(q)
      );
    });
  }, [extensions, searchTerm]);

  return (
    <Widget
      title={t("contextPanel.widgets.extensions")}
      icon={<IconPuzzle className="size-3.5" />}
      flush
    >
      {extensions.length === 0 ? (
        <p className="px-3 py-2.5 text-xs text-foreground-subtle">
          {t("contextPanel.empty.noExtensions")}
        </p>
      ) : (
        <div>
          <div className="border-b border-border px-3 py-1.5">
            <div className="flex items-center gap-1.5 text-foreground-subtle">
              <IconSearch className="size-3" />
              <Input
                variant="ghost"
                value={searchTerm}
                onChange={(e) => setSearchTerm(e.target.value)}
                placeholder={t("contextPanel.widgets.searchExtensions")}
                className="text-xs"
              />
            </div>
          </div>
          <div className="max-h-40 overflow-y-auto px-3 py-2">
            {filtered.length === 0 ? (
              <p className="py-1 text-xs text-foreground-subtle">
                {t("contextPanel.empty.noMatchingExtensions")}
              </p>
            ) : (
              <div className="space-y-2">
                {filtered.map((ext) => (
                  <div key={ext.config_key} className="flex items-center gap-2">
                    <span className="size-1.5 shrink-0 rounded-full bg-green-500" />
                    <span className="truncate text-xs">
                      {getDisplayName(ext)}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </Widget>
  );
}
