import type { AcpProvider } from "@/shared/api/acp";
import type { Persona } from "@/shared/types/agents";
import type { ChatAttachmentDraft } from "@/shared/types/messages";

export interface ModelOption {
  id: string;
  name: string;
  displayName?: string;
  provider?: string;
  providerId?: string;
  providerName?: string;
  contextLimit?: number | null;
  /** Whether this model should appear in the compact recommended picker. */
  recommended?: boolean;
}

export interface ProjectOption {
  id: string;
  name: string;
  workingDirs: string[];
  color?: string | null;
}

export interface ChatInputProps {
  onSend: (
    text: string,
    personaId?: string,
    attachments?: ChatAttachmentDraft[],
  ) => boolean | Promise<boolean>;
  onStop?: () => void;
  isStreaming?: boolean;
  disabled?: boolean;
  queuedMessage?: { text: string } | null;
  onDismissQueue?: () => void;
  initialValue?: string;
  onDraftChange?: (text: string) => void;
  className?: string;
  personas?: Persona[];
  selectedPersonaId?: string | null;
  onPersonaChange?: (personaId: string | null) => void;
  onCreatePersona?: () => void;
  providers?: AcpProvider[];
  providersLoading?: boolean;
  selectedProvider?: string;
  onProviderChange?: (providerId: string) => void;
  currentModelId?: string | null;
  currentModel?: string;
  availableModels?: ModelOption[];
  modelsLoading?: boolean;
  modelStatusMessage?: string | null;
  onModelChange?: (modelId: string) => void;
  selectedProjectId?: string | null;
  availableProjects?: ProjectOption[];
  onProjectChange?: (projectId: string | null) => void;
  onCreateProject?: (options?: {
    onCreated?: (projectId: string) => void;
  }) => void;
  contextTokens?: number;
  contextLimit?: number;
  isContextUsageReady?: boolean;
  onCompactContext?: () => Promise<unknown> | undefined;
  canCompactContext?: boolean;
  isCompactingContext?: boolean;
  supportsCompactionControls?: boolean;
}
