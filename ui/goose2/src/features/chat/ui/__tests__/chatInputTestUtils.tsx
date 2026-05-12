import { ChatInput as BaseChatInput } from "../ChatInput";
import type {
  ChatInputAgentModelPicker,
  ChatInputComposerActions,
  ChatInputContextUsage,
  ChatInputPersonaPicker,
  ChatInputProjectPicker,
  ChatInputProps,
  ChatInputSendHandler,
} from "../../types";

type ChatInputHarnessProps = Omit<
  ChatInputProps,
  | "composerActions"
  | "personaPicker"
  | "agentModelPicker"
  | "projectPicker"
  | "contextUsage"
> &
  Partial<ChatInputComposerActions> &
  ChatInputPersonaPicker &
  ChatInputAgentModelPicker &
  ChatInputProjectPicker &
  ChatInputContextUsage & {
    onSend: ChatInputSendHandler;
  };

export function ChatInput({
  onSend,
  onStop,
  isStreaming,
  disabled,
  queuedMessage,
  onDismissQueue,
  personas,
  selectedPersonaId,
  onPersonaChange,
  providers,
  providersLoading,
  selectedProvider,
  onProviderChange,
  currentModelId,
  currentModelProviderId,
  currentModel,
  availableModels,
  modelsLoading,
  modelStatusMessage,
  onModelChange,
  onPickerOpen,
  selectedProjectId,
  availableProjects,
  onProjectChange,
  onCreateProject,
  contextTokens,
  contextLimit,
  isContextUsageReady,
  onCompactContext,
  canCompactContext,
  isCompactingContext,
  supportsCompactionControls,
  ...props
}: ChatInputHarnessProps) {
  return (
    <BaseChatInput
      {...props}
      composerActions={{
        onSend,
        onStop,
        isStreaming,
        disabled,
        queuedMessage,
        onDismissQueue,
      }}
      personaPicker={{
        personas,
        selectedPersonaId,
        onPersonaChange,
      }}
      agentModelPicker={{
        providers,
        providersLoading,
        selectedProvider,
        onProviderChange,
        currentModelId,
        currentModelProviderId,
        currentModel,
        availableModels,
        modelsLoading,
        modelStatusMessage,
        onModelChange,
        onPickerOpen,
      }}
      projectPicker={{
        selectedProjectId,
        availableProjects,
        onProjectChange,
        onCreateProject,
      }}
      contextUsage={{
        contextTokens,
        contextLimit,
        isContextUsageReady,
        onCompactContext,
        canCompactContext,
        isCompactingContext,
        supportsCompactionControls,
      }}
    />
  );
}
