import { useEffect, useMemo, useState } from "react";
import {
  Mic,
  ArrowUp,
  Square,
  Paperclip,
  File,
  FolderOpen,
  Settings2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLocaleFormatting } from "@/shared/i18n";
import { IconLibraryPlusFilled } from "@tabler/icons-react";
import type { AcpProvider } from "@/shared/api/acp";
import { cn } from "@/shared/lib/cn";
import { ChatInputSelector } from "./ChatInputSelector";
import { ContextRing } from "./ContextRing";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { Progress } from "@/shared/ui/progress";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/shared/ui/tooltip";
import { AgentModelPicker } from "./AgentModelPicker";
import { formatProviderLabel } from "@/shared/ui/icons/ProviderIcons";
import { getCatalogEntryFromEntries } from "@/features/providers/providerCatalog";
import { useProviderCatalogStore } from "@/features/providers/stores/providerCatalogStore";
import { supportsContextCompactionControls } from "../lib/autoCompact";
import { requestOpenSettings } from "@/features/settings/lib/settingsEvents";
import { ProjectSelectorIcon } from "./ProjectSelectorIcon";
import type {
  ChatInputAgentModelPicker,
  ChatInputContextUsage,
  ChatInputPersonaPicker,
  ChatInputProjectPicker,
} from "../types";

const NO_PROJECT_VALUE = "__no_project__";
const CREATE_PROJECT_VALUE = "__create_project__";

interface ChatInputToolbarComposerActions {
  canSend: boolean;
  isStreaming: boolean;
  hasQueuedMessage: boolean;
  onSend: () => void;
  onStop?: () => void;
  onAttachFiles?: () => void;
  onAttachFolders?: () => void;
  disabled?: boolean;
  voiceEnabled?: boolean;
  voiceRecording?: boolean;
  voiceTranscribing?: boolean;
  onVoiceToggle?: () => void;
}

interface ChatInputToolbarProps {
  personaPicker: Pick<ChatInputPersonaPicker, "selectedPersonaId">;
  agentModelPicker: ChatInputAgentModelPicker;
  projectPicker: ChatInputProjectPicker;
  contextUsage: ChatInputContextUsage;
  composerActions: ChatInputToolbarComposerActions;
  isCompact: boolean;
}

export function ChatInputToolbar({
  personaPicker,
  agentModelPicker,
  projectPicker,
  contextUsage,
  composerActions,
  isCompact,
}: ChatInputToolbarProps) {
  const { t } = useTranslation("chat");
  const { formatNumber } = useLocaleFormatting();
  const catalogEntries = useProviderCatalogStore((state) => state.entries);
  const [isContextPopoverOpen, setIsContextPopoverOpen] = useState(false);
  const { selectedPersonaId = null } = personaPicker;
  const {
    providers = [],
    providersLoading,
    selectedProvider = "goose",
    onProviderChange,
    currentModelId,
    currentModelProviderId,
    currentModel,
    availableModels = [],
    modelsLoading = false,
    modelStatusMessage = null,
    onModelChange,
    onPickerOpen,
  } = agentModelPicker;
  const {
    selectedProjectId = null,
    availableProjects = [],
    onProjectChange,
    onCreateProject,
  } = projectPicker;
  const {
    contextTokens = 0,
    contextLimit = 0,
    isContextUsageReady,
    supportsCompactionControls,
    canCompactContext = false,
    isCompactingContext = false,
    onCompactContext,
  } = contextUsage;
  const {
    canSend,
    isStreaming,
    hasQueuedMessage,
    onSend,
    onStop,
    onAttachFiles,
    onAttachFolders,
    disabled = false,
    voiceEnabled = false,
    voiceRecording = false,
    voiceTranscribing = false,
    onVoiceToggle,
  } = composerActions;
  const compactionControlsSupported =
    supportsCompactionControls ??
    supportsContextCompactionControls(selectedProvider);

  const agentProviders = useMemo(() => {
    const seen = new Set<string>();
    const available: AcpProvider[] = [];
    for (const provider of providers) {
      if (seen.has(provider.id)) {
        continue;
      }
      seen.add(provider.id);
      available.push({
        id: provider.id,
        label:
          getCatalogEntryFromEntries(catalogEntries, provider.id)
            ?.displayName ?? provider.label,
      });
    }
    if (available.length > 0) return available;
    return [
      {
        id: selectedProvider,
        label:
          getCatalogEntryFromEntries(catalogEntries, selectedProvider)
            ?.displayName ?? formatProviderLabel(selectedProvider),
      },
    ];
  }, [catalogEntries, providers, selectedProvider]);
  const selectedProject = availableProjects.find(
    (project) => project.id === selectedProjectId,
  );
  const projectLabel = selectedProject?.name ?? t("toolbar.noProject");
  const projectTitle = selectedProject?.workingDirs.length
    ? selectedProject.workingDirs.join(", ")
    : undefined;
  const contextProgress =
    contextLimit > 0 ? Math.min(contextTokens / contextLimit, 1) : 0;
  const showContextUsage = isContextUsageReady ?? contextLimit > 0;
  const contextPercentDigits =
    contextProgress > 0 && contextProgress < 0.1 ? 1 : 0;
  const usedPercentLabel = formatNumber(contextProgress, {
    style: "percent",
    minimumFractionDigits: contextPercentDigits,
    maximumFractionDigits: contextPercentDigits,
  });
  const formatCompactTokenCount = (value: number) =>
    formatNumber(value, {
      notation: "compact",
      compactDisplay: "short",
      maximumFractionDigits: value < 10_000 ? 1 : 0,
    });

  const handleProjectValueChange = (value: string) => {
    if (value === CREATE_PROJECT_VALUE) {
      onCreateProject?.();
      return;
    }

    onProjectChange?.(value === NO_PROJECT_VALUE ? null : value);
  };

  const handleCompactContext = () => {
    if (!canCompactContext || isCompactingContext || !onCompactContext) {
      return;
    }

    setIsContextPopoverOpen(false);
    void onCompactContext();
  };

  const handleOpenAutoCompactSettings = () => {
    setIsContextPopoverOpen(false);
    requestOpenSettings("compaction");
  };

  useEffect(() => {
    if (!showContextUsage && isContextPopoverOpen) {
      setIsContextPopoverOpen(false);
    }
  }, [isContextPopoverOpen, showContextUsage]);

  return (
    <div
      className={cn(
        "flex items-center justify-between gap-2",
        isCompact && "flex-wrap gap-y-2",
      )}
    >
      {/* Left side: pickers */}
      <div
        className={cn(
          "flex min-w-0 items-center gap-0.5",
          isCompact && "flex-1",
        )}
      >
        {(agentProviders.length > 0 || providersLoading) && (
          <AgentModelPicker
            agents={agentProviders}
            selectedAgentId={selectedProvider}
            onAgentChange={(providerId) => onProviderChange?.(providerId)}
            currentModelId={currentModelId}
            currentModelProviderId={currentModelProviderId}
            currentModelName={currentModel ?? null}
            availableModels={availableModels}
            modelsLoading={modelsLoading}
            modelStatusMessage={modelStatusMessage}
            onModelChange={onModelChange}
            onOpen={onPickerOpen}
            loading={providersLoading}
            isCompact={isCompact}
            showSelectedModelInTrigger={selectedPersonaId === null}
          />
        )}

        <ChatInputSelector
          ariaLabel={t("toolbar.selectProject")}
          value={selectedProjectId ?? NO_PROJECT_VALUE}
          triggerLabel={projectLabel}
          triggerTitle={projectTitle}
          icon={<ProjectSelectorIcon icon={selectedProject?.icon} />}
          triggerVariant="toolbar"
          triggerSize="sm"
          menuLabel={t("toolbar.chooseProject")}
          contentWidth="wide"
          sections={[
            {
              items: [
                {
                  value: NO_PROJECT_VALUE,
                  label: t("toolbar.noProject"),
                  description: t("toolbar.generalChatWithoutProject"),
                  icon: <ProjectSelectorIcon />,
                },
                ...availableProjects.map((project) => ({
                  value: project.id,
                  label: project.name,
                  description: project.workingDirs.length
                    ? project.workingDirs.join(", ")
                    : undefined,
                  icon: <ProjectSelectorIcon icon={project.icon} />,
                })),
              ],
            },
            {
              items: [
                ...(onCreateProject
                  ? [
                      {
                        value: CREATE_PROJECT_VALUE,
                        label: t("toolbar.createProject"),
                        icon: (
                          <IconLibraryPlusFilled className="size-4 text-foreground" />
                        ),
                      },
                    ]
                  : []),
              ],
            },
          ].filter((section) => section.items.length > 0)}
          onValueChange={handleProjectValueChange}
        />
      </div>

      {/* Right side: actions */}
      <div className={cn("flex shrink-0 items-center", isCompact && "ml-auto")}>
        <div className="flex items-center gap-px">
          {showContextUsage && (
            <Popover
              open={isContextPopoverOpen}
              onOpenChange={setIsContextPopoverOpen}
            >
              <PopoverTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size={isCompact ? "icon-sm" : "sm"}
                  className={cn(
                    "group rounded-full bg-transparent text-foreground/80 shadow-none hover:bg-transparent hover:text-foreground data-[state=open]:bg-transparent data-[state=open]:text-foreground",
                    isCompact ? "px-0" : "px-2.5",
                  )}
                  aria-label={t("toolbar.contextUsage")}
                  title={t("toolbar.contextUsageTitle", {
                    tokens: formatNumber(contextTokens),
                    limit: formatNumber(contextLimit),
                  })}
                >
                  <ContextRing
                    tokens={contextTokens}
                    limit={contextLimit}
                    size={18}
                  />
                </Button>
              </PopoverTrigger>
              <PopoverContent
                side="top"
                align="end"
                sideOffset={8}
                className="w-60 rounded-2xl p-1 text-left"
              >
                <div className="px-2 py-1.5 text-sm font-semibold text-foreground">
                  {t("toolbar.contextWindow")}
                </div>
                <div className="space-y-2 px-2 pb-1.5">
                  <Progress
                    className="h-1.5 bg-muted"
                    value={contextProgress * 100}
                  />
                  <div className="flex items-center justify-between gap-3 text-xs text-foreground">
                    <div className="truncate">
                      {t("toolbar.contextTokensBreakdown", {
                        tokens: formatCompactTokenCount(contextTokens),
                        limit: formatCompactTokenCount(contextLimit),
                      })}
                    </div>
                    <div className="shrink-0">{usedPercentLabel}</div>
                  </div>
                  {compactionControlsSupported ? (
                    <div className="flex items-center gap-1 pt-0.5">
                      <Button
                        type="button"
                        variant="secondary"
                        size="xs"
                        className="min-w-0 flex-1 justify-center"
                        onClick={handleCompactContext}
                        disabled={!canCompactContext || isCompactingContext}
                      >
                        {isCompactingContext
                          ? t("toolbar.compacting")
                          : t("toolbar.compactNow")}
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        className="shrink-0 rounded-full"
                        onClick={handleOpenAutoCompactSettings}
                        aria-label={t("toolbar.settings")}
                        title={t("toolbar.settings")}
                      >
                        <Settings2 className="size-4" />
                      </Button>
                    </div>
                  ) : null}
                </div>
              </PopoverContent>
            </Popover>
          )}

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                disabled={disabled}
                aria-label={t("toolbar.attach")}
                title={t("toolbar.attach")}
              >
                <Paperclip className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem
                onSelect={() => onAttachFiles?.()}
                disabled={disabled}
              >
                <File className="mr-2 h-4 w-4" />
                {t("toolbar.attachFile")}
              </DropdownMenuItem>
              <DropdownMenuItem
                onSelect={() => onAttachFolders?.()}
                disabled={disabled}
              >
                <FolderOpen className="mr-2 h-4 w-4" />
                {t("toolbar.attachFolder")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>

          <Tooltip>
            <TooltipTrigger asChild>
              <span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  disabled={!voiceRecording && (!voiceEnabled || disabled)}
                  onClick={onVoiceToggle}
                  aria-label={
                    voiceRecording
                      ? t("toolbar.voiceInputRecording")
                      : t("toolbar.voiceInput")
                  }
                  className={cn(
                    voiceRecording &&
                      "bg-destructive/10 text-destructive hover:bg-destructive/20 hover:text-destructive",
                    voiceTranscribing && "animate-pulse",
                  )}
                >
                  <Mic className="h-4 w-4" />
                </Button>
              </span>
            </TooltipTrigger>
            <TooltipContent>
              {!voiceEnabled
                ? t("toolbar.voiceInputDisabled")
                : voiceRecording
                  ? t("toolbar.voiceInputRecording")
                  : voiceTranscribing
                    ? t("toolbar.voiceInputTranscribing")
                    : t("toolbar.voiceInput")}
            </TooltipContent>
          </Tooltip>
        </div>

        <div className="ml-2">
          {isStreaming && !canSend && !hasQueuedMessage ? (
            <Button
              type="button"
              onClick={onStop}
              variant="ghost"
              size="icon-sm"
              className="rounded-full bg-destructive/10 text-destructive hover:bg-destructive/20 hover:text-destructive"
              aria-label={t("toolbar.stopGeneration")}
              title={t("toolbar.stopGeneration")}
            >
              <Square className="h-3.5 w-3.5" />
            </Button>
          ) : (
            <Button
              type="button"
              onClick={onSend}
              disabled={!canSend}
              size="icon-sm"
              className={cn(
                "rounded-full",
                "shadow-none",
                canSend
                  ? "bg-foreground text-background hover:bg-foreground/90"
                  : "cursor-default bg-foreground/10 text-muted-foreground disabled:opacity-100",
              )}
              aria-label={t("toolbar.sendMessage")}
              title={canSend ? t("toolbar.sendMessage") : undefined}
            >
              <ArrowUp className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
