import { useEffect, useState } from "react";
import { IconCheck, IconChevronDown } from "@tabler/icons-react";
import { useTranslation } from "react-i18next";
import type { AcpProvider } from "@/shared/api/acp";
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
import { AllModelsList, RecommendedModelList } from "./AgentModelPickerLists";
import { PickerItem } from "./AgentModelPickerItem";

interface AgentModelPickerProps {
  agents: AcpProvider[];
  selectedAgentId: string;
  onAgentChange: (agentId: string) => void;
  currentModelId?: string | null;
  currentModelProviderId?: string | null;
  currentModelName?: string | null;
  availableModels: ModelOption[];
  modelsLoading?: boolean;
  modelStatusMessage?: string | null;
  onModelChange?: (modelId: string, model?: ModelOption) => void;
  loading?: boolean;
  isCompact?: boolean;
  showSelectedModelInTrigger?: boolean;
  onOpen?: () => void;
}

type ModelView = "recommended" | "all";

export function AgentModelPicker({
  agents,
  selectedAgentId,
  onAgentChange,
  currentModelId = null,
  currentModelProviderId = null,
  currentModelName = null,
  availableModels,
  modelsLoading = false,
  modelStatusMessage = null,
  onModelChange,
  loading = false,
  isCompact = false,
  showSelectedModelInTrigger = true,
  onOpen,
}: AgentModelPickerProps) {
  const { t } = useTranslation("chat");
  const [open, setOpen] = useState(false);
  const [modelView, setModelView] = useState<ModelView>("recommended");
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

  const handleModelSelect = (model: ModelOption) => {
    onModelChange?.(model.id, model);
    setOpen(false);
  };

  // Reset to recommended view when popover closes.
  useEffect(() => {
    if (!open) {
      setModelView("recommended");
    }
  }, [open]);

  // When in "all" view, expand the popover to full width for the search experience.
  const isAllView = modelView === "all";

  return (
    <Popover
      open={open}
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen);
        if (nextOpen) onOpen?.();
      }}
    >
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="toolbar"
          size="sm"
          aria-label={t("toolbar.chooseAgentModel")}
          disabled={loading && !selectedAgentLabel}
          leftIcon={getProviderIcon(selectedAgentId, "size-3.5")}
          rightIcon={<IconChevronDown className="opacity-50" />}
          className="min-w-0 max-w-full"
        >
          <span className={cn("truncate", isCompact ? "max-w-32" : "max-w-56")}>
            {triggerModelLabel ??
              selectedAgentLabel ??
              (loading ? t("toolbar.loading") : null)}
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
                      const agentIcon = getProviderIcon(agent.id, "size-4");

                      return (
                        <PickerItem
                          key={agent.id}
                          onClick={() => handleAgentSelect(agent.id)}
                          selected={isSelected}
                        >
                          {agentIcon ? (
                            <span className="shrink-0">{agentIcon}</span>
                          ) : null}
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
              <div className="flex min-h-0 min-w-0 flex-1 flex-col">
                <div className="shrink-0 px-2 py-1.5 text-sm font-semibold">
                  {t("toolbar.model")}
                </div>
                {currentModelName || currentModelId ? (
                  <div className="space-y-0.5 p-1">
                    <PickerItem selected disabled>
                      <div className="min-w-0 flex-1 truncate">
                        {currentModelName ?? currentModelId}
                      </div>
                      <Spinner className="size-3.5 shrink-0" />
                    </PickerItem>
                  </div>
                ) : (
                  <div className="flex min-h-0 flex-1 items-center gap-2 px-2 py-2 text-sm text-muted-foreground">
                    <Spinner className="size-4" />
                    <span>{t("toolbar.loadingModels")}</span>
                  </div>
                )}
              </div>
            ) : availableModels.length > 0 ? (
              modelView === "recommended" ? (
                <RecommendedModelList
                  models={availableModels}
                  currentModelId={currentModelId}
                  currentModelProviderId={currentModelProviderId}
                  selectedAgentId={selectedAgentId}
                  onModelSelect={handleModelSelect}
                  onShowAll={() => setModelView("all")}
                  t={t}
                />
              ) : (
                <AllModelsList
                  models={availableModels}
                  currentModelId={currentModelId}
                  currentModelProviderId={currentModelProviderId}
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
