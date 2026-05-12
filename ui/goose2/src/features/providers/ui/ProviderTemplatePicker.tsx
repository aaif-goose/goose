import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { SearchBar } from "@/shared/ui/SearchBar";
import { ScrollArea } from "@/shared/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/shared/ui/select";
import { IconLayoutGrid } from "@tabler/icons-react";
import type { CustomProviderEngine } from "@/features/providers/lib/customProviderTypes";
import type { ProviderTemplate } from "./CustomProviderForm";

interface ProviderTemplatePickerProps {
  templates: ProviderTemplate[];
  onSelect: (templateId: string) => void;
  disabled?: boolean;
}

type CompatibilityFilter = "all" | CustomProviderEngine;

function matchesTemplateSearch(template: ProviderTemplate, query: string) {
  const needle = query.trim().toLowerCase();
  if (!needle) {
    return true;
  }

  return [
    template.displayName,
    template.description ?? "",
    template.engine,
    template.id,
    ...template.models,
  ]
    .join(" ")
    .toLowerCase()
    .includes(needle);
}

export function ProviderTemplatePicker({
  templates,
  onSelect,
  disabled = false,
}: ProviderTemplatePickerProps) {
  const { t } = useTranslation("settings");
  const [query, setQuery] = useState("");
  const [compatibility, setCompatibility] =
    useState<CompatibilityFilter>("all");
  const filteredTemplates = useMemo(
    () =>
      templates.filter((template) => {
        const matchesCompatibility =
          compatibility === "all" || template.engine === compatibility;
        return matchesCompatibility && matchesTemplateSearch(template, query);
      }),
    [compatibility, query, templates],
  );

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-[minmax(0,1fr)_12rem]">
        <SearchBar
          value={query}
          onChange={setQuery}
          placeholder={t("providers.custom.templates.searchPlaceholder")}
          size="small"
        />
        <Select
          value={compatibility}
          onValueChange={(value) =>
            setCompatibility(value as CompatibilityFilter)
          }
          disabled={disabled}
        >
          <SelectTrigger size="sm" className="w-full text-xs">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">
              {t("providers.custom.templates.compatibility.all")}
            </SelectItem>
            <SelectItem value="openai_compatible">
              {t("providers.custom.engines.openai_compatible")}
            </SelectItem>
            <SelectItem value="anthropic_compatible">
              {t("providers.custom.engines.anthropic_compatible")}
            </SelectItem>
            <SelectItem value="ollama_compatible">
              {t("providers.custom.engines.ollama_compatible")}
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      <ScrollArea className="h-80 rounded-lg border border-border">
        <div className="space-y-1 p-2">
          {filteredTemplates.map((template) => (
            <button
              key={template.id}
              type="button"
              onClick={() => onSelect(template.id)}
              disabled={disabled}
              className="flex min-h-12 w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm transition-colors hover:bg-accent/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
            >
              <IconLayoutGrid className="size-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">
                  {template.displayName}
                </span>
                <span className="block truncate text-xs text-muted-foreground">
                  {t(`providers.custom.engines.${template.engine}`)}
                  {template.models.length > 0
                    ? ` · ${t("providers.custom.modelCount", {
                        count: template.models.length,
                      })}`
                    : ""}
                </span>
              </span>
            </button>
          ))}

          {filteredTemplates.length === 0 ? (
            <p className="px-2 py-6 text-center text-xs text-muted-foreground">
              {t("providers.custom.templates.empty")}
            </p>
          ) : null}
        </div>
      </ScrollArea>
    </div>
  );
}
