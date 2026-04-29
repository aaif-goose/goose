import { useEffect, useMemo, useRef, useState } from "react";
import { IconCheck, IconChevronLeft, IconSearch } from "@tabler/icons-react";
import { SearchBar } from "@/shared/ui/SearchBar";
import { ScrollArea } from "@/shared/ui/scroll-area";
import {
  formatProviderLabel,
  getProviderIcon,
} from "@/shared/ui/icons/ProviderIcons";
import type { ModelOption } from "../types";
import { PickerItem } from "./AgentModelPickerItem";

function getModelDisplayName(model: ModelOption) {
  return model.displayName ?? model.name;
}

function getGooseModelProviderLabel(model: ModelOption) {
  if (model.providerName) {
    return model.providerName;
  }

  if (model.providerId) {
    return formatProviderLabel(model.providerId);
  }

  return null;
}

function modelMatchesSelection(
  model: ModelOption,
  currentModelId: string | null,
  currentModelProviderId: string | null,
) {
  if (model.id !== currentModelId) {
    return false;
  }

  if (currentModelProviderId) {
    return model.providerId === currentModelProviderId;
  }

  // Providerless selections are ambiguous legacy/incomplete state, so fall back
  // to model-ID-only matching until the user selects a concrete provider row.
  return true;
}

function sortModels(
  models: ModelOption[],
  currentModelId: string | null,
  currentModelProviderId: string | null,
) {
  return [...models].sort((left, right) => {
    if (modelMatchesSelection(left, currentModelId, currentModelProviderId)) {
      return -1;
    }
    if (modelMatchesSelection(right, currentModelId, currentModelProviderId)) {
      return 1;
    }

    const leftProvider = getGooseModelProviderLabel(left) ?? "";
    const rightProvider = getGooseModelProviderLabel(right) ?? "";
    if (leftProvider !== rightProvider) {
      return leftProvider.localeCompare(rightProvider);
    }

    return getModelDisplayName(left).localeCompare(getModelDisplayName(right));
  });
}

interface ModelListProps {
  models: ModelOption[];
  currentModelId: string | null;
  currentModelProviderId: string | null;
  selectedAgentId: string;
  onModelSelect: (model: ModelOption) => void;
  t: (key: string) => string;
}

export function RecommendedModelList({
  models,
  currentModelId,
  currentModelProviderId,
  selectedAgentId,
  onModelSelect,
  onShowAll,
  t,
}: ModelListProps & { onShowAll: () => void }) {
  const recommended = useMemo(() => {
    const rec = models.filter((m) => m.recommended);
    if (
      currentModelId &&
      rec.length > 0 &&
      !rec.some((m) =>
        modelMatchesSelection(m, currentModelId, currentModelProviderId),
      )
    ) {
      const current = models.find((m) =>
        modelMatchesSelection(m, currentModelId, currentModelProviderId),
      );
      if (current) {
        return [current, ...rec];
      }
    }
    return rec.length > 0 ? rec : models;
  }, [models, currentModelId, currentModelProviderId]);

  const sorted = useMemo(
    () => sortModels(recommended, currentModelId, currentModelProviderId),
    [recommended, currentModelId, currentModelProviderId],
  );

  const hasMore = models.length > recommended.length;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="shrink-0 px-2 py-1.5 text-sm font-semibold">
        {t("toolbar.model")}
      </div>
      <ScrollArea className="min-h-0 min-w-0 flex-1">
        <div className="space-y-0.5 p-1">
          {sorted.map((model) => {
            const providerLabel = getGooseModelProviderLabel(model);
            const providerIcon =
              selectedAgentId === "goose" && model.providerId
                ? getProviderIcon(model.providerId, "size-3.5")
                : null;
            const isSelected = modelMatchesSelection(
              model,
              currentModelId,
              currentModelProviderId,
            );
            return (
              <PickerItem
                key={`${model.providerId ?? "model"}:${model.id}`}
                onClick={() => onModelSelect(model)}
                selected={isSelected}
                className="justify-between"
              >
                <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
                  {providerIcon ? (
                    <span
                      className="shrink-0 text-muted-foreground"
                      title={providerLabel ?? undefined}
                    >
                      {providerIcon}
                    </span>
                  ) : null}
                  <div className="min-w-0 flex-1 truncate">
                    {getModelDisplayName(model)}
                  </div>
                </div>
                {isSelected ? (
                  <IconCheck className="size-4 shrink-0 text-muted-foreground" />
                ) : null}
              </PickerItem>
            );
          })}
        </div>
      </ScrollArea>
      {hasMore ? (
        <div className="shrink-0 border-t px-1 py-1">
          <button
            type="button"
            onClick={onShowAll}
            className="flex w-full items-center gap-1.5 rounded-sm px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          >
            <IconSearch className="size-3.5" />
            <span>{t("toolbar.showAllModels")}</span>
          </button>
        </div>
      ) : null}
    </div>
  );
}

export function AllModelsList({
  models,
  currentModelId,
  currentModelProviderId,
  selectedAgentId,
  onModelSelect,
  onBack,
  t,
}: ModelListProps & { onBack: () => void }) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const filtered = useMemo(() => {
    if (!query.trim()) {
      return sortModels(models, currentModelId, currentModelProviderId);
    }
    const q = query.toLowerCase();
    const matches = models.filter(
      (m) =>
        m.name.toLowerCase().includes(q) ||
        m.id.toLowerCase().includes(q) ||
        m.displayName?.toLowerCase().includes(q) ||
        m.providerName?.toLowerCase().includes(q) ||
        m.providerId?.toLowerCase().includes(q),
    );
    return sortModels(matches, currentModelId, currentModelProviderId);
  }, [models, query, currentModelId, currentModelProviderId]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-1 px-1 py-1">
        <button
          type="button"
          onClick={onBack}
          className="flex shrink-0 items-center rounded-sm p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
          aria-label={t("toolbar.model")}
        >
          <IconChevronLeft className="size-4" />
        </button>
        <SearchBar
          inputRef={inputRef}
          size="small"
          value={query}
          onChange={setQuery}
          placeholder={t("toolbar.searchModels")}
          className="min-w-0 flex-1"
        />
      </div>
      {filtered.length > 0 ? (
        <ScrollArea className="min-h-0 min-w-0 flex-1">
          <div className="space-y-0.5 p-1">
            {filtered.map((model) => {
              const providerLabel = getGooseModelProviderLabel(model);
              const providerIcon =
                selectedAgentId === "goose" && model.providerId
                  ? getProviderIcon(model.providerId, "size-3.5")
                  : null;
              const displayName = getModelDisplayName(model);
              const showModelId =
                model.id !== model.name && model.id !== displayName;
              const isSelected = modelMatchesSelection(
                model,
                currentModelId,
                currentModelProviderId,
              );

              return (
                <PickerItem
                  key={`${model.providerId ?? "model"}:${model.id}`}
                  onClick={() => onModelSelect(model)}
                  selected={isSelected}
                  className="justify-between"
                >
                  <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
                    {providerIcon ? (
                      <span
                        className="shrink-0 text-muted-foreground"
                        title={providerLabel ?? undefined}
                      >
                        {providerIcon}
                      </span>
                    ) : null}
                    <div className="min-w-0 flex-1 overflow-hidden">
                      <div className="truncate">{displayName}</div>
                      {showModelId ? (
                        <div className="truncate text-xs text-muted-foreground">
                          {model.id}
                        </div>
                      ) : null}
                    </div>
                  </div>
                  {isSelected ? (
                    <IconCheck className="size-4 shrink-0 text-muted-foreground" />
                  ) : null}
                </PickerItem>
              );
            })}
          </div>
        </ScrollArea>
      ) : (
        <div className="px-3 py-4 text-center text-sm text-muted-foreground">
          {t("toolbar.noSearchResults")}
        </div>
      )}
    </div>
  );
}
