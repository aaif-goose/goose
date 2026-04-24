import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  IconCheck,
  IconChevronDown,
  IconChevronLeft,
  IconSearch,
} from "@tabler/icons-react";
import { useTranslation } from "react-i18next";
import type { AcpProvider } from "@/shared/api/acp";
import { getProviderInventory } from "@/features/providers/api/inventory";
import { useProviderInventoryStore } from "@/features/providers/stores/providerInventoryStore";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { ScrollArea } from "@/shared/ui/scroll-area";
import { Spinner } from "@/shared/ui/spinner";
import {
  formatProviderLabel,
  getProviderIcon,
} from "@/shared/ui/icons/ProviderIcons";
import type { ModelOption } from "../types";

interface AgentModelPickerProps {
  agents: AcpProvider[];
  selectedAgentId: string;
  onAgentChange: (agentId: string) => void;
  currentModelId?: string | null;
  currentModelName?: string | null;
  availableModels: ModelOption[];
  modelsLoading?: boolean;
  modelStatusMessage?: string | null;
  onModelChange?: (modelId: string) => void;
  loading?: boolean;
  isCompact?: boolean;
  showSelectedModelInTrigger?: boolean;
}

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

function sortModels(models: ModelOption[], currentModelId: string | null) {
  return [...models].sort((left, right) => {
    if (left.id === currentModelId) return -1;
    if (right.id === currentModelId) return 1;

    const leftProvider = getGooseModelProviderLabel(left) ?? "";
    const rightProvider = getGooseModelProviderLabel(right) ?? "";
    if (leftProvider !== rightProvider) {
      return leftProvider.localeCompare(rightProvider);
    }

    return getModelDisplayName(left).localeCompare(getModelDisplayName(right));
  });
}

function PickerItem({
  children,
  onClick,
  selected = false,
  disabled = false,
  className,
}: {
  children: ReactNode;
  onClick?: () => void;
  selected?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        "flex min-w-0 w-full items-center gap-2 overflow-hidden rounded-sm px-2 py-1.5 text-left text-sm transition-colors",
        "hover:bg-muted focus-visible:bg-muted focus:outline-none",
        "disabled:pointer-events-none disabled:opacity-50",
        selected && "bg-muted/60",
        className,
      )}
    >
      {children}
    </button>
  );
}

// ── Model list views ────────────────────────────────────────────────

type ModelView = "recommended" | "all";

function RecommendedModelList({
  models,
  currentModelId,
  selectedAgentId,
  onModelSelect,
  onShowAll,
  t,
}: {
  models: ModelOption[];
  currentModelId: string | null;
  selectedAgentId: string;
  onModelSelect: (id: string) => void;
  onShowAll: () => void;
  t: (key: string) => string;
}) {
  const recommended = useMemo(() => {
    const rec = models.filter((m) => m.recommended);
    // If the current model isn't in the recommended list, prepend it
    // so the user can always see what's selected.
    if (
      currentModelId &&
      rec.length > 0 &&
      !rec.some((m) => m.id === currentModelId)
    ) {
      const current = models.find((m) => m.id === currentModelId);
      if (current) {
        return [current, ...rec];
      }
    }
    // Fall back to full list if no recommendations exist (e.g. ACP agents).
    return rec.length > 0 ? rec : models;
  }, [models, currentModelId]);

  const sorted = useMemo(
    () => sortModels(recommended, currentModelId),
    [recommended, currentModelId],
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
            return (
              <PickerItem
                key={`${model.providerId ?? "model"}:${model.id}`}
                onClick={() => onModelSelect(model.id)}
                selected={model.id === currentModelId}
                className="justify-between"
              >
                <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
                  {selectedAgentId === "goose" && model.providerId ? (
                    <span
                      className="shrink-0 text-muted-foreground"
                      title={providerLabel ?? undefined}
                    >
                      {getProviderIcon(model.providerId, "size-3.5")}
                    </span>
                  ) : null}
                  <div className="min-w-0 flex-1 truncate">
                    {getModelDisplayName(model)}
                  </div>
                </div>
                {model.id === currentModelId ? (
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

function AllModelsList({
  models,
  currentModelId,
  selectedAgentId,
  onModelSelect,
  onBack,
  t,
}: {
  models: ModelOption[];
  currentModelId: string | null;
  selectedAgentId: string;
  onModelSelect: (id: string) => void;
  onBack: () => void;
  t: (key: string) => string;
}) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    // Auto-focus search on mount.
    inputRef.current?.focus();
  }, []);

  const filtered = useMemo(() => {
    if (!query.trim()) {
      return sortModels(models, currentModelId);
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
    return sortModels(matches, currentModelId);
  }, [models, query, currentModelId]);

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
        <div className="relative min-w-0 flex-1">
          <IconSearch className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("toolbar.searchModels")}
            className="h-7 w-full rounded-sm border bg-transparent pl-7 pr-2 text-sm outline-none placeholder:text-muted-foreground focus:ring-1 focus:ring-ring"
          />
        </div>
      </div>
      {filtered.length > 0 ? (
        <ScrollArea className="min-h-0 min-w-0 flex-1">
          <div className="space-y-0.5 p-1">
            {filtered.map((model) => {
              const providerLabel = getGooseModelProviderLabel(model);
              const displayName = getModelDisplayName(model);
              // Show the raw model_id as secondary text when it differs from name
              const showModelId =
                model.id !== model.name && model.id !== displayName;

              return (
                <PickerItem
                  key={`${model.providerId ?? "model"}:${model.id}`}
                  onClick={() => onModelSelect(model.id)}
                  selected={model.id === currentModelId}
                  className="justify-between"
                >
                  <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
                    {selectedAgentId === "goose" && model.providerId ? (
                      <span
                        className="shrink-0 text-muted-foreground"
                        title={providerLabel ?? undefined}
                      >
                        {getProviderIcon(model.providerId, "size-3.5")}
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
                  {model.id === currentModelId ? (
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

// ── Main component ──────────────────────────────────────────────────

export function AgentModelPicker({
  agents,
  selectedAgentId,
  onAgentChange,
  currentModelId = null,
  currentModelName = null,
  availableModels,
  modelsLoading = false,
  modelStatusMessage = null,
  onModelChange,
  loading = false,
  isCompact = false,
  showSelectedModelInTrigger = true,
}: AgentModelPickerProps) {
  const { t } = useTranslation("chat");
  const [open, setOpen] = useState(false);
  const [modelView, setModelView] = useState<ModelView>("recommended");
  const mergeInventoryEntries = useProviderInventoryStore(
    (s) => s.mergeEntries,
  );

  const selectedAgentLabel =
    agents.find((agent) => agent.id === selectedAgentId)?.label ??
    formatProviderLabel(selectedAgentId);
  const hasSelectedModel =
    showSelectedModelInTrigger &&
    (currentModelName !== null || currentModelId !== null);
  const triggerModelLabel = hasSelectedModel
    ? (currentModelName ?? currentModelId)
    : null;

  const handleAgentSelect = (agentId: string) => {
    if (agentId !== selectedAgentId) {
      onAgentChange(agentId);
      setModelView("recommended");
    }
  };

  const handleModelSelect = (modelId: string) => {
    onModelChange?.(modelId);
    setOpen(false);
  };

  // Reset to recommended view when popover closes.
  useEffect(() => {
    if (!open) {
      setModelView("recommended");
    }
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    let cancelled = false;

    const syncInventory = async () => {
      try {
        const entries = await getProviderInventory();
        if (cancelled) {
          return;
        }
        mergeInventoryEntries(entries);
      } catch (error) {
        console.error("Failed to sync provider inventory from picker:", error);
      }
    };

    void syncInventory();

    return () => {
      cancelled = true;
    };
  }, [open, mergeInventoryEntries]);

  // When in "all" view, expand the popover to full width for the search experience.
  const isAllView = modelView === "all";

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="toolbar"
          size="sm"
          aria-label={t("toolbar.chooseAgentModel")}
          disabled={loading}
          leftIcon={getProviderIcon(selectedAgentId, "size-3.5")}
          rightIcon={<IconChevronDown className="opacity-50" />}
          className="min-w-0"
        >
          <span className={cn("truncate", isCompact ? "max-w-32" : "max-w-56")}>
            {loading
              ? t("toolbar.loading")
              : (triggerModelLabel ?? selectedAgentLabel)}
          </span>
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="h-[min(24rem,50vh)] w-96 overflow-hidden p-1"
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            const col = (document.activeElement as HTMLElement)?.closest(
              "[data-col]",
            );
            if (!col) return;
            const items = Array.from(
              col.querySelectorAll<HTMLElement>("button:not(:disabled)"),
            );
            const idx = items.indexOf(document.activeElement as HTMLElement);
            const next =
              e.key === "ArrowDown"
                ? items[(idx + 1) % items.length]
                : items[(idx - 1 + items.length) % items.length];
            next?.focus();
          } else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
            e.preventDefault();
            const content = e.currentTarget as HTMLElement;
            const cols = Array.from(
              content.querySelectorAll<HTMLElement>("[data-col]"),
            );
            const currentCol = (document.activeElement as HTMLElement)?.closest(
              "[data-col]",
            );
            const colIdx = cols.indexOf(currentCol as HTMLElement);
            const targetCol =
              e.key === "ArrowRight"
                ? cols[(colIdx + 1) % cols.length]
                : cols[(colIdx - 1 + cols.length) % cols.length];
            if (!targetCol) return;
            const targetItems = Array.from(
              targetCol.querySelectorAll<HTMLElement>("button:not(:disabled)"),
            );
            const currentItems = Array.from(
              currentCol?.querySelectorAll<HTMLElement>(
                "button:not(:disabled)",
              ) ?? [],
            );
            const currentIdx = currentItems.indexOf(
              document.activeElement as HTMLElement,
            );
            const target =
              targetItems[Math.min(currentIdx, targetItems.length - 1)] ??
              targetItems[0];
            target?.focus();
          }
        }}
      >
        <div
          className={cn(
            "grid h-full gap-1 overflow-hidden",
            isAllView
              ? "grid-cols-1"
              : "grid-cols-[minmax(0,1fr)_minmax(0,1fr)]",
          )}
        >
          {/* Agent column — hidden in "all models" search view */}
          {!isAllView ? (
            <div
              data-col="agent"
              className="flex min-h-0 min-w-0 overflow-hidden p-1"
            >
              <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                <div className="shrink-0 px-2 py-1.5 text-sm font-semibold">
                  {t("toolbar.agent")}
                </div>
                <ScrollArea className="min-h-0 min-w-0 flex-1">
                  <div className="space-y-0.5 p-1">
                    {agents.map((agent) => {
                      const isSelected = agent.id === selectedAgentId;

                      return (
                        <PickerItem
                          key={agent.id}
                          onClick={() => handleAgentSelect(agent.id)}
                          selected={isSelected}
                        >
                          <span className="shrink-0">
                            {getProviderIcon(agent.id, "size-4")}
                          </span>
                          <span className="min-w-0 flex-1 truncate">
                            {agent.label}
                          </span>
                          {isSelected ? (
                            <IconCheck className="size-4 shrink-0 text-muted-foreground" />
                          ) : null}
                        </PickerItem>
                      );
                    })}
                  </div>
                </ScrollArea>
              </div>
            </div>
          ) : null}

          {/* Model column */}
          <div
            data-col="model"
            className="flex min-h-0 min-w-0 overflow-hidden p-1"
          >
            {modelsLoading ? (
              <div className="flex min-h-0 flex-1 items-center gap-2 px-2 py-2 text-sm text-muted-foreground">
                <Spinner className="size-4" />
                <span>{t("toolbar.loadingModels")}</span>
              </div>
            ) : availableModels.length > 0 ? (
              modelView === "recommended" ? (
                <RecommendedModelList
                  models={availableModels}
                  currentModelId={currentModelId}
                  selectedAgentId={selectedAgentId}
                  onModelSelect={handleModelSelect}
                  onShowAll={() => setModelView("all")}
                  t={t}
                />
              ) : (
                <AllModelsList
                  models={availableModels}
                  currentModelId={currentModelId}
                  selectedAgentId={selectedAgentId}
                  onModelSelect={handleModelSelect}
                  onBack={() => setModelView("recommended")}
                  t={t}
                />
              )
            ) : (
              <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                <div className="shrink-0 px-2 py-1.5 text-sm font-semibold">
                  {t("toolbar.model")}
                </div>
                <div className="px-2 py-2">
                  <div className="text-sm text-muted-foreground">
                    {modelStatusMessage ??
                      currentModelName ??
                      t("toolbar.noModelsAvailable")}
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
