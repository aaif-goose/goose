import { useMemo } from "react";
import {
  Mic,
  ArrowUp,
  Square,
  Paperclip,
  File,
  FolderOpen,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useLocaleFormatting } from "@/shared/i18n";
import { IconLibraryPlusFilled } from "@tabler/icons-react";
import type { AcpProvider } from "@/shared/api/acp";
import type { Persona } from "@/shared/types/agents";
import { cn } from "@/shared/lib/cn";
import { ChatInputSelector } from "./ChatInputSelector";
import { ContextRing } from "./ContextRing";
import { PersonaPicker } from "./PersonaPicker";
import type { ProjectOption } from "./ChatInput";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/shared/ui/tooltip";
import { AgentModelPicker } from "./AgentModelPicker";
import type { ModelOption } from "../types";
import { formatProviderLabel } from "@/shared/ui/icons/ProviderIcons";
import { useAgentProviderStatus } from "@/features/providers/hooks/useAgentProviderStatus";
import {
  getCatalogEntry,
  resolveAgentProviderCatalogIdStrict,
} from "@/features/providers/providerCatalog";

const NO_PROJECT_VALUE = "__no_project__";
const CREATE_PROJECT_VALUE = "__create_project__";

function ProjectDot({ color }: { color?: string | null }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-block size-2 rounded-full",
        color ? "" : "bg-muted-foreground/40",
      )}
      style={color ? { backgroundColor: color } : undefined}
    />
  );
}

interface ChatInputToolbarProps {
  // Personas
  personas: Persona[];
  selectedPersonaId: string | null;
  onPersonaChange?: (personaId: string | null) => void;
  onCreatePersona?: () => void;
  // Provider
  providers: AcpProvider[];
  providersLoading?: boolean;
  selectedProvider: string;
  onProviderChange: (providerId: string) => void;
  // Model
  currentModelId?: string | null;
  currentModel?: string;
  availableModels: ModelOption[];
  onModelChange?: (modelId: string) => void;
  // Project
  selectedProjectId: string | null;
  availableProjects: ProjectOption[];
  onProjectChange?: (projectId: string | null) => void;
  onCreateProject?: (options?: {
    onCreated?: (projectId: string) => void;
  }) => void;
  // Context
  contextTokens: number;
  contextLimit: number;
  // Actions
  canSend: boolean;
  isStreaming: boolean;
  hasQueuedMessage: boolean;
  onSend: () => void;
  onStop?: () => void;
  onAttachFiles?: () => void;
  onAttachFolders?: () => void;
  disabled?: boolean;
  // Layout
  isCompact: boolean;
}

export function ChatInputToolbar({
  personas,
  selectedPersonaId,
  onPersonaChange,
  onCreatePersona,
  providers,
  providersLoading,
  selectedProvider,
  onProviderChange,
  currentModelId,
  currentModel,
  availableModels,
  onModelChange,
  selectedProjectId,
  availableProjects,
  onProjectChange,
  onCreateProject,
  contextTokens,
  contextLimit,
  canSend,
  isStreaming,
  hasQueuedMessage,
  onSend,
  onStop,
  onAttachFiles,
  onAttachFolders,
  disabled = false,
  isCompact,
}: ChatInputToolbarProps) {
  const { t } = useTranslation("chat");
  const { formatNumber } = useLocaleFormatting();
  const { readyAgentIds } = useAgentProviderStatus();

  const agentProviders = useMemo(() => {
    const seen = new Set<string>();
    const connected: AcpProvider[] = [];
    for (const p of providers) {
      const catalogId = resolveAgentProviderCatalogIdStrict(p.id);
      if (
        catalogId === null ||
        !readyAgentIds.has(catalogId) ||
        seen.has(catalogId)
      )
        continue;
      seen.add(catalogId);
      connected.push({
        id: p.id,
        label: getCatalogEntry(catalogId)?.displayName ?? p.label,
      });
    }
    if (connected.length > 0) return connected;
    return [
      {
        id: selectedProvider,
        label:
          getCatalogEntry(selectedProvider)?.displayName ??
          formatProviderLabel(selectedProvider),
      },
    ];
  }, [providers, readyAgentIds, selectedProvider]);
  const selectedProject = availableProjects.find(
    (project) => project.id === selectedProjectId,
  );
  const projectLabel = selectedProject?.name ?? t("toolbar.noProject");
  const projectTitle = selectedProject?.workingDirs.length
    ? selectedProject.workingDirs.join(", ")
    : undefined;

  const handleProjectValueChange = (value: string) => {
    if (value === CREATE_PROJECT_VALUE) {
      onCreateProject?.();
      return;
    }

    onProjectChange?.(value === NO_PROJECT_VALUE ? null : value);
  };

  return (
    <div className="flex items-center justify-between gap-2">
      {/* Left side: pickers */}
      <div className="flex items-center gap-0.5">
        {(agentProviders.length > 0 || providersLoading) && (
          <AgentModelPicker
            agents={agentProviders}
            selectedAgentId={selectedProvider}
            onAgentChange={onProviderChange}
            currentModelId={currentModelId}
            currentModelName={currentModel ?? null}
            availableModels={availableModels}
            onModelChange={onModelChange}
            loading={providersLoading}
            isCompact={isCompact}
          />
        )}

        <ChatInputSelector
          ariaLabel={t("toolbar.selectProject")}
          value={selectedProjectId ?? NO_PROJECT_VALUE}
          triggerLabel={projectLabel}
          triggerTitle={projectTitle}
          icon={<ProjectDot color={selectedProject?.color} />}
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
                  icon: <ProjectDot />,
                },
                ...availableProjects.map((project) => ({
                  value: project.id,
                  label: project.name,
                  description: project.workingDirs.length
                    ? project.workingDirs.join(", ")
                    : undefined,
                  icon: <ProjectDot color={project.color} />,
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
      <div className="flex items-center">
        <div className="flex items-center gap-px">
          {personas.length > 0 && (
            <PersonaPicker
              personas={personas}
              selectedPersonaId={selectedPersonaId}
              onPersonaChange={(id) => onPersonaChange?.(id)}
              onCreatePersona={onCreatePersona}
              triggerVariant="icon"
            />
          )}

          {contextLimit > 0 && (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              className="rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground"
              aria-label={t("toolbar.contextUsage")}
              title={t("toolbar.contextUsageTitle", {
                tokens: formatNumber(contextTokens),
                limit: formatNumber(contextLimit),
              })}
            >
              <ContextRing tokens={contextTokens} limit={contextLimit} />
            </Button>
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
                  disabled
                  aria-label={t("toolbar.voiceInputSoon")}
                >
                  <Mic />
                </Button>
              </span>
            </TooltipTrigger>
            <TooltipContent>{t("toolbar.voiceInputSoon")}</TooltipContent>
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
