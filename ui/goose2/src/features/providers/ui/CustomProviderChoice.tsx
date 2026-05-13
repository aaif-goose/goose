import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { IconPencil, IconTrash } from "@tabler/icons-react";

export interface CustomProviderChoiceInfo {
  providerId: string;
  displayName: string;
  description?: string;
  modelCount: number;
  configured: boolean;
}

interface CustomProviderChoiceProps {
  provider: CustomProviderChoiceInfo;
  onEdit: () => void;
  onDelete: () => void;
  deleting?: boolean;
}

export function CustomProviderChoice({
  provider,
  onEdit,
  onDelete,
  deleting = false,
}: CustomProviderChoiceProps) {
  const { t } = useTranslation("settings");

  return (
    <div className="flex items-center gap-3 rounded-lg border border-border px-3 py-2.5">
      <div className="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground">
        {provider.displayName.charAt(0).toUpperCase()}
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <p className="truncate text-sm">{provider.displayName}</p>
          {!provider.configured ? (
            <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-xxs text-muted-foreground">
              {t("providers.custom.notConfigured")}
            </span>
          ) : null}
        </div>
      </div>

      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={t("providers.custom.actions.editProvider", {
          name: provider.displayName,
        })}
        onClick={onEdit}
      >
        <IconPencil className="size-3.5" />
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        aria-label={t("providers.custom.actions.deleteProvider", {
          name: provider.displayName,
        })}
        onClick={onDelete}
        disabled={deleting}
        className="text-danger hover:text-danger"
      >
        <IconTrash className="size-3.5" />
      </Button>
    </div>
  );
}
