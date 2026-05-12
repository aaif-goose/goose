import { useState, useRef, useCallback, useEffect, useMemo } from "react";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  attachmentSnapshotsMatch,
  skillDraftSnapshotsMatch,
} from "../lib/chatInputSnapshots";
import {
  getChatInputAgentLabel,
  getChatInputPlaceholder,
} from "../lib/chatInputPlaceholder";
import { cn } from "@/shared/lib/cn";
import { Badge } from "@/shared/ui/badge";
import { Popover, PopoverAnchor } from "@/shared/ui/popover";
import { MentionAutocomplete } from "./MentionAutocomplete";
import { useMentionHandlers } from "../hooks/useMentionHandlers";
import { ChatInputToolbar } from "./ChatInputToolbar";
import { formatProviderLabel } from "@/shared/ui/icons/ProviderIcons";
import { TooltipProvider } from "@/shared/ui/tooltip";
import { useAttachmentDropTarget } from "../hooks/useAttachmentDropTarget";
import { useChatInputAttachments } from "../hooks/useChatInputAttachments";
import { useChatInputFilePicker } from "../hooks/useChatInputFilePicker";
import { ChatInputAttachments } from "./ChatInputAttachments";
import { ChatInputSelectionChips } from "./ChatInputSelectionChips";
import { useChatInputSubmit } from "../hooks/useChatInputSubmit";
import { useVoiceDictation } from "../hooks/useVoiceDictation";
import { resolveDisplayModelLabel } from "../lib/modelDisplayLabel";
import type { ChatInputProps, ChatSkillDraft } from "../types";

export function ChatInput({
  composerActions,
  initialValue = "",
  onDraftChange,
  selectedSkills: selectedSkillsProp,
  onSkillsChange,
  className,
  personaPicker,
  agentModelPicker,
  projectPicker,
  contextUsage,
}: ChatInputProps) {
  const {
    onSend,
    onStop,
    isStreaming = false,
    disabled = false,
    queuedMessage = null,
    onDismissQueue,
  } = composerActions;
  const {
    personas = [],
    selectedPersonaId = null,
    onPersonaChange,
  } = personaPicker ?? {};
  const {
    providers = [],
    providersLoading = false,
    selectedProvider = "goose",
    onProviderChange,
    currentModelId = null,
    currentModelProviderId = null,
    currentModel,
    availableModels = [],
    modelsLoading = false,
    modelStatusMessage = null,
    onModelChange,
    onPickerOpen,
  } = agentModelPicker ?? {};
  const {
    selectedProjectId = null,
    availableProjects = [],
    onProjectChange,
    onCreateProject,
  } = projectPicker ?? {};
  const {
    contextTokens = 0,
    contextLimit = 0,
    isContextUsageReady,
    onCompactContext,
    canCompactContext = false,
    isCompactingContext = false,
    supportsCompactionControls,
  } = contextUsage ?? {};
  const { t } = useTranslation("chat");
  const [text, setTextRaw] = useState(initialValue);
  const [internalSelectedSkills, setInternalSelectedSkills] = useState<
    ChatSkillDraft[]
  >([]);
  const selectedSkills = selectedSkillsProp ?? internalSelectedSkills;
  const setSelectedSkills = onSkillsChange ?? setInternalSelectedSkills;
  const textRef = useRef(initialValue);
  useEffect(() => {
    setTextRaw(initialValue);
    textRef.current = initialValue;
  }, [initialValue]);
  const setText = useCallback(
    (value: string) => {
      textRef.current = value;
      setTextRaw(value);
      onDraftChange?.(value);
    },
    [onDraftChange],
  );
  const [isCompact, setIsCompact] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const {
    attachments,
    addBrowserFiles,
    addPathAttachments,
    removeAttachment,
    clearAttachments,
  } = useChatInputAttachments();
  const attachmentsRef = useRef(attachments);
  attachmentsRef.current = attachments;
  const selectedSkillsRef = useRef(selectedSkills);
  selectedSkillsRef.current = selectedSkills;

  const resetTextarea = useCallback(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, []);

  const hasQueuedMessage = queuedMessage !== null;

  const activePersona = useMemo(
    () => personas.find((persona) => persona.id === selectedPersonaId) ?? null,
    [personas, selectedPersonaId],
  );
  const selectedProject = useMemo(
    () =>
      availableProjects.find((project) => project.id === selectedProjectId) ??
      null,
    [availableProjects, selectedProjectId],
  );
  const stickyPersona = activePersona;

  const canSend =
    (text.trim().length > 0 ||
      attachments.length > 0 ||
      selectedSkills.length > 0) &&
    !hasQueuedMessage &&
    !disabled;

  const handleSkillMentionAdded = useCallback(
    (skill: (typeof selectedSkills)[number]) => {
      if (
        selectedSkills.some((selectedSkill) => selectedSkill.id === skill.id)
      ) {
        return;
      }
      setSelectedSkills([...selectedSkills, skill]);
    },
    [selectedSkills, setSelectedSkills],
  );

  const {
    mentionOpen,
    mentionSelectedIndex,
    filteredPersonas,
    filteredSkills,
    filteredFiles,
    resolveSkillSlashCommand,
    detectMention,
    closeMention,
    navigateMention,
    confirmMention,
    handlePersonaMentionSelect,
    handleSkillMentionSelect,
    handleFileMentionSelect,
    handleMentionConfirm,
  } = useMentionHandlers({
    personas,
    projectWorkingDirs: selectedProject?.workingDirs,
    text,
    setText,
    textareaRef,
    onPersonaChange,
    onSkillMentionSelect: handleSkillMentionAdded,
  });

  useEffect(() => {
    const element = containerRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setIsCompact(entry.contentRect.width < 580);
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => textareaRef.current?.focus(), []);

  const { submitChatInputMessage, handleVoiceAutoSubmit } = useChatInputSubmit({
    attachmentsRef,
    selectedSkillsRef,
    selectedPersonaId,
    onSend,
    setSelectedSkills,
    resolveSkillSlashCommand,
  });

  const dictation = useVoiceDictation({
    text,
    setText,
    attachments,
    clearAttachments,
    selectedPersonaId,
    onSend,
    onAutoSubmit: handleVoiceAutoSubmit,
    resetTextarea,
    isSendLocked: hasQueuedMessage || disabled,
  });

  const handleSend = useCallback(async () => {
    if (!canSend) {
      return;
    }

    // Stop without flushing so Send uses the text already in the composer.
    // This also cancels an in-flight microphone startup.
    if (
      dictation.isRecording ||
      dictation.isTranscribing ||
      dictation.isStarting()
    ) {
      dictation.stopRecording({ flushPending: false });
    }

    const submittedText = text;
    const submittedSkills = selectedSkills;
    const submittedAttachments = attachments;
    const accepted = await submitChatInputMessage(
      submittedText,
      submittedAttachments,
      submittedSkills,
    );
    if (!accepted) {
      return;
    }
    const draftStillMatchesSubmission =
      textRef.current === submittedText &&
      skillDraftSnapshotsMatch(selectedSkillsRef.current, submittedSkills) &&
      attachmentSnapshotsMatch(attachmentsRef.current, submittedAttachments);
    if (!draftStillMatchesSubmission) {
      return;
    }
    setText("");
    setSelectedSkills([]);
    clearAttachments();
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [
    attachments,
    canSend,
    clearAttachments,
    dictation,
    selectedSkills,
    setSelectedSkills,
    setText,
    submitChatInputMessage,
    text,
  ]);

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (mentionOpen) {
      if (event.key === "Escape") {
        event.preventDefault();
        closeMention();
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        navigateMention(event.key === "ArrowDown" ? "down" : "up");
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        const item = confirmMention();
        if (item) {
          event.preventDefault();
          handleMentionConfirm(item);
          return;
        }
      }
    }
    if (event.key === "Enter" && !event.shiftKey && !event.altKey) {
      event.preventDefault();
      void handleSend();
    }
  };

  const handleInput = (event: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = event.target.value;
    setText(value);
    const cursorPosition = event.target.selectionStart ?? value.length;
    detectMention(value, cursorPosition);
    const textarea = event.target;
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
  };

  const handlePaste = useCallback(
    (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
      const files = Array.from(event.clipboardData.items)
        .filter(
          (item) => item.kind === "file" && item.type.startsWith("image/"),
        )
        .map((item) => item.getAsFile())
        .filter((file): file is File => Boolean(file));

      if (files.length === 0) {
        return;
      }

      event.preventDefault();
      void addBrowserFiles(files);
    },
    [addBrowserFiles],
  );

  const {
    isAttachmentDragOver,
    handleDragEnter,
    handleDragOver,
    handleDragLeave,
    handleDrop,
  } = useAttachmentDropTarget({
    disabled,
    isStreaming,
    targetRef: containerRef,
    onDropFiles: (files) => {
      void addBrowserFiles(files);
    },
    onDropPaths: (paths) => {
      void addPathAttachments(paths);
    },
  });
  const { handleAttachFiles, handleAttachFolders } = useChatInputFilePicker({
    disabled,
    addPathAttachments,
  });

  const providerDisplayName =
    providers.find((provider) => provider.id === selectedProvider)?.label ??
    formatProviderLabel(selectedProvider);
  const agentDisplayName = getChatInputAgentLabel(
    activePersona?.displayName,
    providerDisplayName,
  );
  const resolvedCurrentModel = useMemo(() => {
    return (
      resolveDisplayModelLabel({
        currentModelId,
        currentModelName: currentModel,
        currentModelProviderId,
        availableModels,
      }) ?? undefined
    );
  }, [availableModels, currentModel, currentModelId, currentModelProviderId]);
  const inputPlaceholder = getChatInputPlaceholder(
    t,
    agentDisplayName,
    dictation.isRecording,
    dictation.isTranscribing,
  );

  const handleClearStickyPersona = useCallback(() => {
    onPersonaChange?.(null);
  }, [onPersonaChange]);

  const handleRemoveSkill = useCallback(
    (skillId: string) => {
      setSelectedSkills(selectedSkills.filter((skill) => skill.id !== skillId));
    },
    [selectedSkills, setSelectedSkills],
  );

  return (
    <TooltipProvider delayDuration={300}>
      <div
        className={cn(
          "relative z-10 px-2 pb-3 pt-0 sm:px-4 sm:pb-6",
          className,
        )}
      >
        <div className="mx-auto max-w-3xl">
          <Popover open={mentionOpen}>
            {/* biome-ignore lint/a11y/noStaticElementInteractions: drop zone for file attachments */}
            <div
              ref={containerRef}
              className={cn(
                "relative rounded-2xl border border-border bg-background px-3 pb-3 pt-4 transition-colors sm:px-4",
                isAttachmentDragOver && "bg-muted/20",
              )}
              onDragEnter={handleDragEnter}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
            >
              {isAttachmentDragOver && (
                <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-2xl border border-dashed border-border bg-background/70">
                  <Badge
                    variant="secondary"
                    className="px-3 py-1 text-sm shadow-sm"
                  >
                    {t("attachments.dropToAttach")}
                  </Badge>
                </div>
              )}

              <MentionAutocomplete
                filteredPersonas={filteredPersonas}
                filteredSkills={filteredSkills}
                filteredFiles={filteredFiles}
                isOpen={mentionOpen}
                onSelectPersona={handlePersonaMentionSelect}
                onSelectSkill={handleSkillMentionSelect}
                onSelectFile={handleFileMentionSelect}
                onClose={closeMention}
                selectedIndex={mentionSelectedIndex}
              />

              <ChatInputAttachments
                attachments={attachments}
                onRemove={removeAttachment}
              />

              <ChatInputSelectionChips
                persona={stickyPersona}
                skills={selectedSkills}
                onClearPersona={handleClearStickyPersona}
                onRemoveSkill={handleRemoveSkill}
              />

              {queuedMessage && (
                <div className="mb-2 flex items-center gap-2 rounded-lg bg-muted/60 px-3 py-1.5">
                  <span className="flex-1 truncate text-xs text-muted-foreground">
                    {t("queue.label", { text: queuedMessage.text })}
                  </span>
                  <button
                    type="button"
                    onClick={onDismissQueue}
                    className="shrink-0 rounded p-0.5 text-muted-foreground hover:text-foreground"
                    aria-label={t("queue.dismiss")}
                  >
                    <X className="size-3.5" />
                  </button>
                </div>
              )}

              <PopoverAnchor asChild>
                <textarea
                  ref={textareaRef}
                  value={text}
                  onChange={handleInput}
                  onKeyDown={handleKeyDown}
                  onPaste={handlePaste}
                  placeholder={inputPlaceholder}
                  disabled={disabled}
                  rows={1}
                  className="mb-3 min-h-[36px] max-h-[200px] w-full resize-none bg-transparent px-1 text-[14px] leading-relaxed text-foreground placeholder:font-light placeholder:text-muted-foreground/60 focus:outline-none focus-visible:ring-0 focus-visible:ring-offset-0 disabled:opacity-60"
                  aria-label={t("input.ariaLabel")}
                />
              </PopoverAnchor>

              <ChatInputToolbar
                personaPicker={{ selectedPersonaId }}
                agentModelPicker={{
                  providers,
                  providersLoading,
                  selectedProvider,
                  onProviderChange,
                  currentModelId,
                  currentModelProviderId,
                  currentModel: resolvedCurrentModel,
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
                composerActions={{
                  canSend,
                  isStreaming,
                  hasQueuedMessage,
                  onAttachFiles: handleAttachFiles,
                  onAttachFolders: handleAttachFolders,
                  disabled,
                  onSend: handleSend,
                  onStop,
                  voiceEnabled: dictation.isEnabled,
                  voiceRecording: dictation.isRecording,
                  voiceTranscribing: dictation.isTranscribing,
                  onVoiceToggle: dictation.toggleRecording,
                }}
                isCompact={isCompact}
              />
            </div>
          </Popover>
        </div>
      </div>
    </TooltipProvider>
  );
}
