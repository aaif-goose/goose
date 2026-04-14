import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { IconPuzzle } from "@tabler/icons-react";
import { Widget } from "./Widget";
import { listExtensions } from "@/features/extensions/api/extensions";
import type { ExtensionEntry } from "@/features/extensions/types";

export function ExtensionsWidget() {
  const { t } = useTranslation("chat");
  const [extensions, setExtensions] = useState<ExtensionEntry[]>([]);

  useEffect(() => {
    listExtensions()
      .then((all) => setExtensions(all.filter((e) => e.enabled)))
      .catch(() => setExtensions([]));
  }, []);

  return (
    <Widget
      title={t("contextPanel.widgets.extensions")}
      icon={<IconPuzzle className="size-3.5" />}
    >
      {extensions.length === 0 ? (
        <p className="text-foreground-subtle">
          {t("contextPanel.empty.noExtensions")}
        </p>
      ) : (
        <div className="space-y-1">
          {extensions.map((ext) => (
            <div key={ext.name} className="flex items-center gap-2">
              <span className="size-1.5 shrink-0 rounded-full bg-green-500" />
              <span className="truncate text-xs">
                {ext.type === "builtin" && ext.display_name
                  ? ext.display_name
                  : ext.name}
              </span>
            </div>
          ))}
        </div>
      )}
    </Widget>
  );
}
