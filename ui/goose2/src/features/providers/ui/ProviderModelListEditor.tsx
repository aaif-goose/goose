import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { IconPlus, IconX } from "@tabler/icons-react";

interface ProviderModelListEditorProps {
  models: string[];
  onChange: (models: string[]) => void;
  disabled?: boolean;
}

function normalizeModels(values: string[]) {
  return [...new Set(values.map((value) => value.trim()).filter(Boolean))];
}

function splitModelInput(value: string) {
  return value.split(/[\n,]/);
}

export function ProviderModelListEditor({
  models,
  onChange,
  disabled = false,
}: ProviderModelListEditorProps) {
  const { t } = useTranslation("settings");
  const [draft, setDraft] = useState("");

  function addModels(value: string) {
    const nextModels = normalizeModels([...models, ...splitModelInput(value)]);
    onChange(nextModels);
    setDraft("");
  }

  function removeModel(model: string) {
    onChange(models.filter((item) => item !== model));
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <Input
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) {
              return;
            }
            if (event.key === "Enter" || event.key === ",") {
              event.preventDefault();
              addModels(draft);
            }
          }}
          onPaste={(event) => {
            const pasted = event.clipboardData.getData("text");
            if (/[\n,]/.test(pasted)) {
              event.preventDefault();
              addModels(pasted);
            }
          }}
          placeholder={t("providers.custom.fields.modelsPlaceholder")}
          disabled={disabled}
          spellCheck={false}
          className="h-8 text-xs"
        />
        <Button
          type="button"
          size="icon-sm"
          variant="outline"
          aria-label={t("providers.custom.actions.addModel")}
          onClick={() => addModels(draft)}
          disabled={disabled || !draft.trim()}
        >
          <IconPlus className="size-3.5" />
        </Button>
      </div>

      {models.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {models.map((model) => (
            <span
              key={model}
              className="inline-flex h-7 max-w-full items-center gap-1 rounded-full border border-border bg-muted px-2 text-xs"
            >
              <span className="truncate">{model}</span>
              <button
                type="button"
                aria-label={t("providers.custom.actions.removeModel", {
                  model,
                })}
                onClick={() => removeModel(model)}
                disabled={disabled}
                className="rounded-full text-muted-foreground hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
              >
                <IconX className="size-3" />
              </button>
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}
