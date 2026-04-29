import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { IconLayoutGrid, IconSettings } from "@tabler/icons-react";
import type { ProviderTemplate } from "./CustomProviderForm";

interface ProviderTemplatePickerProps {
  templates: ProviderTemplate[];
  selectedTemplateId: string | null;
  onSelect: (templateId: string | null) => void;
  disabled?: boolean;
}

export function ProviderTemplatePicker({
  templates,
  selectedTemplateId,
  onSelect,
  disabled = false,
}: ProviderTemplatePickerProps) {
  const { t } = useTranslation("settings");

  return (
    <div className="space-y-2">
      <div className="grid grid-cols-2 gap-2">
        <button
          type="button"
          onClick={() => onSelect(null)}
          disabled={disabled}
          aria-pressed={selectedTemplateId === null}
          className="flex min-h-16 items-start gap-2 rounded-lg border border-border px-3 py-2 text-left text-sm transition-colors hover:bg-accent/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 aria-pressed:border-ring aria-pressed:bg-accent/25"
        >
          <IconSettings className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0">
            <span className="block font-medium">
              {t("providers.custom.templates.manual")}
            </span>
            <span className="block text-xs text-muted-foreground">
              {t("providers.custom.templates.manualDescription")}
            </span>
          </span>
        </button>

        {templates.map((template) => (
          <button
            key={template.id}
            type="button"
            onClick={() => onSelect(template.id)}
            disabled={disabled}
            aria-pressed={selectedTemplateId === template.id}
            className="flex min-h-16 items-start gap-2 rounded-lg border border-border px-3 py-2 text-left text-sm transition-colors hover:bg-accent/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 aria-pressed:border-ring aria-pressed:bg-accent/25"
          >
            <IconLayoutGrid className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0">
              <span className="block truncate font-medium">
                {template.displayName}
              </span>
              {template.description ? (
                <span className="line-clamp-2 text-xs text-muted-foreground">
                  {template.description}
                </span>
              ) : null}
            </span>
          </button>
        ))}
      </div>

      {templates.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("providers.custom.templates.empty")}
        </p>
      ) : null}

      {selectedTemplateId !== null ? (
        <Button
          type="button"
          variant="ghost"
          size="xs"
          onClick={() => onSelect(null)}
          disabled={disabled}
          className="text-muted-foreground"
        >
          {t("providers.custom.templates.clear")}
        </Button>
      ) : null}
    </div>
  );
}
