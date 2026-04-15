import { useState } from "react";
import { useTranslation } from "react-i18next";
import { IconSettings } from "@tabler/icons-react";
import { Button } from "@/shared/ui/button";
import { Switch } from "@/shared/ui/switch";
import { getDisplayName, type ExtensionEntry } from "../types";

interface ExtensionItemProps {
  extension: ExtensionEntry;
  onToggle: (extension: ExtensionEntry) => Promise<void>;
  onConfigure?: (extension: ExtensionEntry) => void;
}

function getSubtitle(ext: ExtensionEntry): string {
  if (ext.description) return ext.description;
  if (ext.type === "stdio") return ext.cmd;
  if (ext.type === "streamable_http") return ext.uri;
  return ext.type;
}

const EDITABLE_TYPES = new Set(["stdio", "streamable_http"]);

function isEditable(ext: ExtensionEntry): boolean {
  return EDITABLE_TYPES.has(ext.type) && !ext.bundled;
}

export function ExtensionItem({
  extension,
  onToggle,
  onConfigure,
}: ExtensionItemProps) {
  const { t } = useTranslation("settings");
  const [isToggling, setIsToggling] = useState(false);
  const [visualEnabled, setVisualEnabled] = useState(extension.enabled);

  const handleToggle = async () => {
    if (isToggling) return;
    setIsToggling(true);
    setVisualEnabled(!extension.enabled);
    try {
      await onToggle(extension);
    } catch {
      setVisualEnabled(extension.enabled);
    } finally {
      setIsToggling(false);
    }
  };

  const editable = isEditable(extension);
  const checked = isToggling ? visualEnabled : extension.enabled;
  const displayName = getDisplayName(extension);

  return (
    <div className="flex items-center justify-between gap-3 py-4">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{displayName}</span>
          <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
            {t(`extensions.types.${extension.type}`, {
              defaultValue: extension.type,
            })}
          </span>
        </div>
        <p className="mt-0.5 truncate text-xs text-muted-foreground">
          {getSubtitle(extension)}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {editable && onConfigure && (
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => onConfigure(extension)}
            aria-label={t("extensions.configure", {
              name: displayName,
            })}
          >
            <IconSettings className="size-4" />
          </Button>
        )}
        <Switch
          checked={checked}
          onCheckedChange={handleToggle}
          disabled={isToggling}
          aria-label={t("extensions.toggle", {
            name: displayName,
          })}
        />
      </div>
    </div>
  );
}
