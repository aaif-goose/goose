import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { IconPlus, IconTrash } from "@tabler/icons-react";
import { createCustomProviderHeaderDraft } from "@/features/providers/lib/customProviderHeaders";

export interface CustomHeader {
  id: string;
  key: string;
  value: string;
}

interface CustomHeadersEditorProps {
  headers: CustomHeader[];
  onChange: (headers: CustomHeader[]) => void;
  disabled?: boolean;
}

export function CustomHeadersEditor({
  headers,
  onChange,
  disabled = false,
}: CustomHeadersEditorProps) {
  const { t } = useTranslation("settings");

  function updateHeader(
    index: number,
    field: keyof CustomHeader,
    value: string,
  ) {
    onChange(
      headers.map((header, currentIndex) =>
        currentIndex === index ? { ...header, [field]: value } : header,
      ),
    );
  }

  function removeHeader(index: number) {
    onChange(headers.filter((_, currentIndex) => currentIndex !== index));
  }

  return (
    <div className="space-y-2">
      {headers.length > 0 ? (
        <div className="space-y-2">
          {headers.map((header, index) => (
            <div
              key={header.id}
              className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_2rem] gap-2"
            >
              <Input
                value={header.key}
                onChange={(event) =>
                  updateHeader(index, "key", event.target.value)
                }
                placeholder={t("providers.custom.fields.headerKey")}
                disabled={disabled}
                spellCheck={false}
                className="h-8 text-xs"
              />
              <Input
                value={header.value}
                onChange={(event) =>
                  updateHeader(index, "value", event.target.value)
                }
                placeholder={t("providers.custom.fields.headerValue")}
                disabled={disabled}
                spellCheck={false}
                className="h-8 text-xs"
              />
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={t("providers.custom.actions.removeHeader")}
                onClick={() => removeHeader(index)}
                disabled={disabled}
              >
                <IconTrash className="size-3.5" />
              </Button>
            </div>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">
          {t("providers.custom.emptyHeaders")}
        </p>
      )}

      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() =>
          onChange([...headers, createCustomProviderHeaderDraft()])
        }
        disabled={disabled}
        leftIcon={<IconPlus />}
      >
        {t("providers.custom.actions.addHeader")}
      </Button>
    </div>
  );
}
