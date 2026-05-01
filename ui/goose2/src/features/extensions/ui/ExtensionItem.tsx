import { useTranslation } from "react-i18next";
import { IconSettings } from "@tabler/icons-react";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { getDisplayName, type ExtensionEntry } from "../types";

interface ExtensionItemProps {
  extension: ExtensionEntry;
  onConfigure?: (extension: ExtensionEntry) => void;
  className?: string;
}

function getSubtitle(ext: ExtensionEntry): string {
  if (ext.description) return ext.description;
  if (ext.type === "stdio") return ext.cmd;
  if (ext.type === "streamable_http") return ext.uri;
  return ext.type;
}

function isUserManagedExtension(ext: ExtensionEntry): boolean {
  return (
    (ext.type === "stdio" || ext.type === "streamable_http") && !ext.bundled
  );
}

function isEditable(ext: ExtensionEntry): boolean {
  return isUserManagedExtension(ext);
}

export function ExtensionItem({
  extension,
  onConfigure,
  className,
}: ExtensionItemProps) {
  const { t } = useTranslation("settings");
  const editable = isEditable(extension);
  const displayName = getDisplayName(extension);

  return (
    <div
      className={cn(
        "flex min-h-20 items-center justify-between gap-3 border-b border-border-soft-divider py-4",
        className,
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{displayName}</span>
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
      </div>
    </div>
  );
}
