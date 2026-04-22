import { useState, useRef, useCallback, useEffect, useMemo } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/lib/cn";
import { isPromiseLike } from "@/shared/lib/isPromiseLike";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverAnchor } from "@/shared/ui/popover";
import { MentionAutocomplete } from "./MentionAutocomplete";
import { useMentionHandlers } from "../hooks/useMentionHandlers";
import { ChatInputToolbar } from "./ChatInputToolbar";
import { formatProviderLabel } from "@/shared/ui/icons/ProviderIcons";
import { TooltipProvider } from "@/shared/ui/tooltip";
import { PersonaAvatar } from "./PersonaPicker";
import { useAttachmentDropTarget } from "../hooks/useAttachmentDropTarget";
import {
  normalizeDialogSelection,
  useChatInputAttachments,
} from "../hooks/useChatInputAttachments";
import { ChatInputAttachments } from "./ChatInputAttachments";
import { useVoiceDictation } from "../hooks/useVoiceDictation";
import type { ChatAttachmentDraft } from "@/shared/types/messages";
import type { ChatInputProps } from "../types";

function attachmentSnapshotsMatch(
  current: ChatAttachmentDraft[],
  snapshot: ChatAttachmentDraft[],
) {
  return (
    current.length === snapshot.length &&
    current.every((attachment, index) => attachment.id === snapshot[index]?.id)
  );
}

export function ChatInput({
  onSend,
  onStop,
  isStreaming = false,
  disabled = false,
  queuedMessage = null,
  onDismissQueue,
  initialValue = "",
  onDraftChange,
  className,
  personas = [],
  selectedPersonaId = null,
  onPersonaChange,
  onCreatePersona,
  providers = [],
  providersLoading = false,
  selectedProvider = "goose",
  onProviderChange,
  currentModelId = null,
  currentModel,
  availableModels = [],
  modelsLoading = false,
  modelStatusMessage = null,
  onModelChange,
  selectedProjectId = null,
  availableProjects = [],
  onProjectChange,
  onCreateProject,
  contextTokens = 0,
  contextLimit = 0,
  isContextUsageReady,
  onCompactContext,
  canCompactContext = false,
  isCompactingContext = false,
  supportsCompactionControls,
}: ChatInputProps) {
  const { t } = useTranslation("chat");
  const [text, setTextRaw] = useState(initialValue);
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

  const resetTextarea = useCallback(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, []);

  const hasQueuedMessage = queuedMessage !== null;

  const dictation = useVoiceDictation({
    text,
    setText,
    attachments,
    clearAttachments,
    selectedPersonaId,
    onSend,
    resetTextarea,
    isSendLocked: hasQueuedMessage || disabled,
  });

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
    (text.trim().length > 0 || attachments.length > 0) &&
    !hasQueuedMessage &&
    !disabled;

  const {
    mentionOpen,
    mentionSelectedIndex,
    filteredPersonas,
    filteredFiles,
    detectMention,
    closeMention,
    navigateMention,
    confirmMention,
    handlePersonaMentionSelect,
    handleFileMentionSelect,
    handleMentionConfirm,
  } = useMentionHandlers({
    personas,
    projectWorkingDirs: selectedProject?.workingDirs,
    text,
    setText,
    textareaRef,
    onPersonaChange,
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

  const handleSend = useCallback(async () => {
    if (!canSend) {
      return;
    }

    // If recording, stop without waiting for final flush and send what's
    // already transcribed into the textarea. This makes Send a single click
    // even while the mic is hot; any in-flight audio after the user clicked
    // Send is intentionally dropped.
    //
    // Also handles the edge case where the user clicks Send while a
    // getUserMedia startup is still pending (isRecording is still false but
    // a stream is about to be acquired) — stopRecording sets the internal
    // cancel flag so the pending startup tears itself down instead of
    // leaving the OS mic indicator on.
    if (
      dictation.isRecording ||
      dictation.isTranscribing ||
      dictation.isStarting()
    ) {
      dictation.stopRecording({ flushPending: false });
    }

    const submittedText = text;
    const submittedAttachments = attachments;
    const sendResult = onSend(
      submittedText.trim(),
      selectedPersonaId ?? undefined,
      submittedAttachments.length > 0 ? submittedAttachments : undefined,
    );
    const accepted = isPromiseLike<boolean>(sendResult)
      ? await sendResult
      : sendResult;
    if (accepted === false) {
      return;
    }
    const draftStillMatchesSubmission =
      textRef.current === submittedText &&
      attachmentSnapshotsMatch(attachmentsRef.current, submittedAttachments);
    if (!draftStillMatchesSubmission) {
      return;
    }
    setText("");
    clearAttachments();
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [
    attachments,
    canSend,
    clearAttachments,
    dictation,
    onSend,
    selectedPersonaId,
    setText,
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

  const handleAttachFiles = useCallback(async () => {
    if (disabled) {
      return;
    }

    try {
      const selected = await open({
        title: t("attachments.chooseFilesDialogTitle"),
        multiple: true,
      });
      await addPathAttachments(normalizeDialogSelection(selected));
    } catch {
      // Dialog plugin may be unavailable in some environments.
    }
  }, [addPathAttachments, disabled, t]);

  const handleAttachFolders = useCallback(async () => {
    if (disabled) {
      return;
    }

    try {
      const selected = await open({
        directory: true,
        title: t("attachments.chooseFoldersDialogTitle"),
        multiple: true,
      });
      await addPathAttachments(normalizeDialogSelection(selected));
    } catch {
      // Dialog plugin may be unavailable in some environments.
    }
  }, [addPathAttachments, disabled, t]);

  const providerDisplayName =
    providers.find((provider) => provider.id === selectedProvider)?.label ??
    formatProviderLabel(selectedProvider);
  const agentDisplayName = activePersona?.displayName ?? providerDisplayName;
  const resolvedCurrentModel = useMemo(() => {
    if (currentModel) {
      return currentModel;
    }
    if (!currentModelId) {
      return undefined;
    }
    const selectedModel = availableModels.find(
      (model) => model.id === currentModelId,
    );
    return selectedModel?.displayName ?? selectedModel?.name ?? currentModelId;
  }, [availableModels, currentModel, currentModelId]);
  const effectivePlaceholder = t("input.placeholder", {
    agent: agentDisplayName,
  });

  const handleClearStickyPersona = useCallback(() => {
    onPersonaChange?.(null);
  }, [onPersonaChange]);

  return (
    <TooltipProvider delayDuration={300}>
      <div className={cn("px-4 pb-6 pt-2", className)}>
        <div className="mx-auto max-w-3xl">
          <Popover open={mentionOpen}>
            {/* biome-ignore lint/a11y/noStaticElementInteractions: drop zone for file attachments */}
            <div
              ref={containerRef}
              className={cn(
                "relative rounded-2xl border border-border bg-background px-4 pb-3 pt-4 transition-colors",
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
                filteredFiles={filteredFiles}
                isOpen={mentionOpen}
                onSelectPersona={handlePersonaMentionSelect}
                onSelectFile={handleFileMentionSelect}
                onClose={closeMention}
                selectedIndex={mentionSelectedIndex}
              />

              <ChatInputAttachments
                attachments={attachments}
                onRemove={removeAttachment}
              />

              {stickyPersona && (
                <div className="mb-2 flex items-center gap-1.5">
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-brand/10 px-2.5 py-1 text-[11px] font-medium text-brand">
                    <PersonaAvatar persona={stickyPersona} size="sm" />
                    <span>@{stickyPersona.displayName}</span>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-xs"
                      className="ml-0.5 size-auto p-0 opacity-60 hover:bg-transparent hover:opacity-100"
                      onClick={handleClearStickyPersona}
                      aria-label={t("persona.clearActive")}
                    >
                      <X className="size-3" />
                    </Button>
                  </span>
                </div>
              )}

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
                  placeholder={
                    dictation.isRecording
                      ? t("toolbar.voiceInputRecording")
                      : dictation.isTranscribing
                        ? t("toolbar.voiceInputTranscribing")
                        : effectivePlaceholder
                  }
                  disabled={disabled}
                  rows={1}
                  className="mb-3 min-h-[36px] max-h-[200px] w-full resize-none bg-transparent px-1 text-[14px] leading-relaxed text-foreground placeholder:font-light placeholder:text-muted-foreground/60 focus:outline-none focus-visible:ring-0 focus-visible:ring-offset-0 disabled:opacity-60"
                  aria-label={t("input.ariaLabel")}
                />
              </PopoverAnchor>

              <ChatInputToolbar
                personas={personas}
                selectedPersonaId={selectedPersonaId}
                onPersonaChange={onPersonaChange}
                onCreatePersona={onCreatePersona}
                providers={providers}
                providersLoading={providersLoading}
                selectedProvider={selectedProvider}
                onProviderChange={(id) => onProviderChange?.(id)}
                currentModelId={currentModelId}
                currentModel={resolvedCurrentModel}
                availableModels={availableModels}
                modelsLoading={modelsLoading}
                modelStatusMessage={modelStatusMessage}
                onModelChange={onModelChange}
                selectedProjectId={selectedProjectId}
                availableProjects={availableProjects}
                onProjectChange={onProjectChange}
                onCreateProject={onCreateProject}
                contextTokens={contextTokens}
                contextLimit={contextLimit}
                isContextUsageReady={isContextUsageReady}
                onCompactContext={onCompactContext}
                canCompactContext={canCompactContext}
                isCompactingContext={isCompactingContext}
                supportsCompactionControls={supportsCompactionControls}
                canSend={canSend}
                isStreaming={isStreaming}
                hasQueuedMessage={hasQueuedMessage}
                onAttachFiles={handleAttachFiles}
                onAttachFolders={handleAttachFolders}
                disabled={disabled}
                onSend={handleSend}
                onStop={onStop}
                isCompact={isCompact}
                voiceEnabled={dictation.isEnabled}
                voiceRecording={dictation.isRecording}
                voiceTranscribing={dictation.isTranscribing}
                onVoiceToggle={dictation.toggleRecording}
              />
            </div>
          </Popover>
        </div>
      </div>
    </TooltipProvider>
  );
}
