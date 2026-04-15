import { AppEvents } from '../constants/events';
import React, { useRef, useState, useEffect, useMemo, useCallback } from 'react';
import { Bug, ChefHat, ScrollText } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/Tooltip';
import { Button } from './ui/button';
import type { View } from '../utils/navigationUtils';
import Stop from './ui/Stop';
import { Attach, Send, Close, Microphone } from './icons';
import { ChatState } from '../types/chatState';
import debounce from 'lodash/debounce';
import { LocalMessageStorage } from '../utils/localMessageStorage';
import { DirSwitcher } from './bottom_menu/DirSwitcher';
import ModelsBottomBar from './settings/models/bottom_bar/ModelsBottomBar';
import { BottomMenuModeSelection } from './bottom_menu/BottomMenuModeSelection';
import { BottomMenuExtensionSelection } from './bottom_menu/BottomMenuExtensionSelection';
import { AlertType, useAlerts } from './alerts';
import { useConfig } from './ConfigContext';
import { useModelAndProvider } from './ModelAndProviderContext';
import { useAudioRecorder } from '../hooks/useAudioRecorder';
import { toastError } from '../toasts';
import MentionPopover, { DisplayItemWithMatch } from './MentionPopover';
import { COST_TRACKING_ENABLED } from '../updates';
import { CostTracker } from './bottom_menu/CostTracker';
import { ContextWindowIndicator } from './bottom_menu/ContextWindowIndicator';
import { DroppedFile, useFileDrop } from '../hooks/useFileDrop';
import { Recipe } from '../recipe';
import { MessageQueue, QueuedMessage } from './MessageQueue';
import { detectInterruption } from '../utils/interruptionDetector';
import { DiagnosticsModal } from './ui/Diagnostics';
import { getSession, Message } from '../api';
import CreateRecipeFromSessionModal from './recipes/CreateRecipeFromSessionModal';
import CreateEditRecipeModal from './recipes/CreateEditRecipeModal';
import { getInitialWorkingDir } from '../utils/workingDir';
import { getPredefinedModelsFromEnv } from './settings/models/predefinedModelsUtils';
import {
  trackFileAttached,
  trackVoiceDictation,
  trackDiagnosticsOpened,
  trackCreateRecipeOpened,
  trackEditRecipeOpened,
} from '../utils/analytics';
import { getNavigationShortcutText } from '../utils/keyboardShortcuts';
import { UserInput, ImageData } from '../types/message';
import { compressImageDataUrl } from '../utils/conversionUtils';
import { fetchCanonicalModelInfo } from '../utils/canonical';
import { defineMessages, useIntl } from '../i18n';

interface PastedImage {
  id: string;
  dataUrl: string;
  isLoading: boolean;
  error?: string;
}

const MAX_IMAGES_PER_MESSAGE = 10;

// Constants for token and tool alerts
const TOKEN_LIMIT_DEFAULT = 128000; // fallback for custom models that the backend doesn't know about
const TOOLS_MAX_SUGGESTED = 60; // max number of tools before we show a warning

// Manual compact trigger message - must match backend constant
const MANUAL_COMPACT_TRIGGER = '/compact';

const i18n = defineMessages({
  dictationError: {
    id: 'chatInput.dictationError',
    defaultMessage: 'Dictation Error',
  },
  removeImage: {
    id: 'chatInput.removeImage',
    defaultMessage: 'Remove image',
  },
  removeFile: {
    id: 'chatInput.removeFile',
    defaultMessage: 'Remove file',
  },
  unknownType: {
    id: 'chatInput.unknownType',
    defaultMessage: 'Unknown type',
  },
  contextWindow: {
    id: 'chatInput.contextWindow',
    defaultMessage: 'Context window',
  },
  tooManyTools: {
    id: 'chatInput.tooManyTools',
    defaultMessage: 'Too many tools can degrade performance.\nTool count: {toolCount} (recommend: {recommended})',
  },
  viewExtensions: {
    id: 'chatInput.viewExtensions',
    defaultMessage: 'View extensions',
  },
  waitingForImages: {
    id: 'chatInput.waitingForImages',
    defaultMessage: 'Waiting for images to save...',
  },
  processingDroppedFiles: {
    id: 'chatInput.processingDroppedFiles',
    defaultMessage: 'Processing dropped files...',
  },
  recording: {
    id: 'chatInput.recording',
    defaultMessage: 'Recording...',
  },
  transcribing: {
    id: 'chatInput.transcribing',
    defaultMessage: 'Transcribing...',
  },
  restartingSession: {
    id: 'chatInput.restartingSession',
    defaultMessage: 'Restarting session...',
  },
  typeMessage: {
    id: 'chatInput.typeMessage',
    defaultMessage: 'Type a message to send',
  },
  send: {
    id: 'chatInput.send',
    defaultMessage: 'Send',
  },
  failedToReadImage: {
    id: 'chatInput.failedToReadImage',
    defaultMessage: 'Failed to read image file',
  },
  viewEditRecipe: {
    id: 'chatInput.viewEditRecipe',
    defaultMessage: 'View/Edit Recipe',
  },
  createRecipeFromSession: {
    id: 'chatInput.createRecipeFromSession',
    defaultMessage: 'Create Recipe from Session',
  },
});

interface ChatInputProps {
  sessionId: string | null;
  handleSubmit: (input: UserInput) => void;
  chatState: ChatState;
  setChatState?: (state: ChatState) => void;
  onStop?: () => void;
  commandHistory?: string[];
  initialValue?: string;
  droppedFiles?: DroppedFile[];
  onFilesProcessed?: () => void;
  setView: (view: View) => void;
  totalTokens?: number;
  accumulatedInputTokens?: number;
  accumulatedOutputTokens?: number;
  messages?: Message[];
  sessionCosts?: {
    [key: string]: {
      inputTokens: number;
      outputTokens: number;
      totalCost: number;
    };
  };
  disableAnimation?: boolean;
  recipe?: Recipe | null;
  recipeId?: string | null;
  recipeAccepted?: boolean;
  initialPrompt?: string;
  toolCount: number;
  append?: (message: Message) => void;
  onWorkingDirChange?: (newDir: string) => void;
  inputRef?: React.RefObject<HTMLTextAreaElement | null>;
  sessionModel?: string | null;
  sessionProvider?: string | null;
  sessionLoaded?: boolean;
}

export default function ChatInput({
  sessionId,
  handleSubmit,
  chatState = ChatState.Idle,
  setChatState,
  onStop,
  commandHistory = [],
  initialValue = '',
  droppedFiles = [],
  onFilesProcessed,
  setView,
  totalTokens,
  accumulatedInputTokens,
  accumulatedOutputTokens,
  messages = [],
  disableAnimation = false,
  sessionCosts,
  recipe,
  recipeId,
  recipeAccepted,
  initialPrompt,
  toolCount,
  append: _append,
  onWorkingDirChange,
  inputRef,
  sessionModel,
  sessionProvider,
  sessionLoaded,
}: ChatInputProps) {
  const [_value, setValue] = useState(initialValue);
  const [displayValue, setDisplayValue] = useState(initialValue); // For immediate visual feedback
  const [isFocused, setIsFocused] = useState(false);
  const [pastedImages, setPastedImages] = useState<PastedImage[]>([]);
  const [isFilePickerOpen, setIsFilePickerOpen] = useState(false);

  // Derived state - chatState != Idle means we're in some form of loading state
  const isLoading = chatState !== ChatState.Idle;
  const wasLoadingRef = useRef(isLoading);

  // Queue functionality - ephemeral, only exists in memory for this chat instance
  const [queuedMessages, setQueuedMessages] = useState<QueuedMessage[]>([]);
  const queuePausedRef = useRef(false);
  const editingMessageIdRef = useRef<string | null>(null);
  const [lastInterruption, setLastInterruption] = useState<string | null>(null);

  const { alerts, addAlert, clearAlerts } = useAlerts();
  const dropdownRef: React.RefObject<HTMLDivElement> = useRef<HTMLDivElement>(
    null
  ) as React.RefObject<HTMLDivElement>;
  const intl = useIntl();
  const { getProviders } = useConfig();
  const {
    getCurrentModelAndProvider,
    currentModel: configModel,
    currentProvider: configProvider,
  } = useModelAndProvider();

  // Local override for when the user changes the model in the modal,
  // before the session object is re-fetched from the backend.
  const [modelOverride, setModelOverride] = useState<{ model: string; provider: string } | null>(
    null
  );
  const effectiveModel = modelOverride?.model ?? sessionModel ?? configModel;
  const effectiveProvider = modelOverride?.provider ?? sessionProvider ?? configProvider;

  // Clear override when the underlying data catches up (session props for
  // active chats, config defaults for Hub / no-session contexts).
  useEffect(() => {
    if (!modelOverride) return;
    const sessionCaughtUp =
      sessionModel === modelOverride.model && sessionProvider === modelOverride.provider;
    const configCaughtUp =
      !sessionId &&
      configModel === modelOverride.model &&
      configProvider === modelOverride.provider;
    if (sessionCaughtUp || configCaughtUp) {
      setModelOverride(null);
    }
  }, [sessionModel, sessionProvider, configModel, configProvider, sessionId, modelOverride]);
  const [tokenLimit, setTokenLimit] = useState<number>(TOKEN_LIMIT_DEFAULT);
  const [isTokenLimitLoaded, setIsTokenLimitLoaded] = useState(false);
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [showCreateRecipeModal, setShowCreateRecipeModal] = useState(false);
  const [showEditRecipeModal, setShowEditRecipeModal] = useState(false);
  const [sessionWorkingDir, setSessionWorkingDir] = useState<string | null>(null);

  useEffect(() => {
    if (!sessionId) {
      return;
    }

    const fetchSessionWorkingDir = async () => {
      try {
        const response = await getSession({ path: { session_id: sessionId } });
        if (response.data?.working_dir) {
          setSessionWorkingDir(response.data.working_dir);
        }
      } catch (error) {
        console.error('[ChatInput] Failed to fetch session working dir:', error);
      }
    };

    fetchSessionWorkingDir();
  }, [sessionId]);

  // Save queue state (paused/interrupted) to storage
  useEffect(() => {
    try {
      window.sessionStorage.setItem('goose-queue-paused', JSON.stringify(queuePausedRef.current));
    } catch (error) {
      console.error('Error saving queue pause state:', error);
    }
  }, [queuedMessages]); // Save when queue changes

  useEffect(() => {
    try {
      window.sessionStorage.setItem('goose-queue-interruption', JSON.stringify(lastInterruption));
    } catch (error) {
      console.error('Error saving queue interruption state:', error);
    }
  }, [lastInterruption]);

  // Cleanup effect - save final state on component unmount
  useEffect(() => {
    return () => {
      // Save final queue state when component unmounts
      try {
        window.sessionStorage.setItem('goose-queue-paused', JSON.stringify(queuePausedRef.current));
        window.sessionStorage.setItem('goose-queue-interruption', JSON.stringify(lastInterruption));
      } catch (error) {
        console.error('Error saving queue state on unmount:', error);
      }
    };
  }, [lastInterruption]); // Include lastInterruption in dependency array

  // Queue processing
  useEffect(() => {
    if (wasLoadingRef.current && !isLoading && queuedMessages.length > 0) {
      // After an interruption, we should process the interruption message immediately
      // The queue is only truly paused if there was an interruption AND we want to keep it paused
      const shouldProcessQueue = !queuePausedRef.current || lastInterruption;

      if (shouldProcessQueue) {
        const nextMessage = queuedMessages[0];
        LocalMessageStorage.addMessage(nextMessage.content);
        handleSubmit({ msg: nextMessage.content, images: nextMessage.images });
        setQueuedMessages((prev) => {
          const newQueue = prev.slice(1);
          // If queue becomes empty after processing, clear the paused state
          if (newQueue.length === 0) {
            queuePausedRef.current = false;
            setLastInterruption(null);
          }
          return newQueue;
        });

        // Clear the interruption flag after processing the interruption message
        if (lastInterruption) {
          setLastInterruption(null);
          // Keep the queue paused after sending the interruption message
          // User can manually resume if they want to continue with queued messages
          queuePausedRef.current = true;
        }
      }
    }
    wasLoadingRef.current = isLoading;
  }, [isLoading, queuedMessages, handleSubmit, lastInterruption]);
  const [mentionPopover, setMentionPopover] = useState<{
    isOpen: boolean;
    position: { x: number; y: number };
    query: string;
    mentionStart: number;
    selectedIndex: number;
    isSlashCommand: boolean;
  }>({
    isOpen: false,
    position: { x: 0, y: 0 },
    query: '',
    mentionStart: -1,
    selectedIndex: 0,
    isSlashCommand: false,
  });
  const mentionPopoverRef = useRef<{
    getDisplayFiles: () => DisplayItemWithMatch[];
    selectFile: (index: number) => void;
  }>(null);

  // Audio recorder hook for voice dictation
  const {
    isEnabled,
    dictationProvider,
    isRecording,
    isTranscribing,
    startRecording,
    stopRecording,
  } = useAudioRecorder({
    onTranscription: (text) => {
      trackVoiceDictation('transcribed');

      let filteredText = text.replace(/\([^)]*\)/g, '').trim();

      if (!filteredText) {
        return;
      }

      const shouldAutoSubmit = /\bsubmit[.,!?;'"\s]*$/i.test(filteredText);

      const cleanedText = shouldAutoSubmit
        ? filteredText.replace(/\bsubmit[.,!?;'"\s]*$/i, '').trim()
        : filteredText;

      const newValue =
        displayValue.trim() && cleanedText
          ? `${displayValue.trim()} ${cleanedText}`
          : displayValue.trim() || cleanedText;

      setDisplayValue(newValue);
      setValue(newValue);

      if (shouldAutoSubmit && newValue.trim()) {
        trackVoiceDictation('auto_submit');
        setTimeout(() => {
          performSubmit(newValue);
        }, 100);
      } else {
        textAreaRef.current?.focus();
      }
    },
    onError: (message) => {
      const errorType = 'DictationError';
      trackVoiceDictation('error', undefined, errorType);
      toastError({
        title: intl.formatMessage(i18n.dictationError),
        msg: message,
      });
    },
  });
  const internalTextAreaRef = useRef<HTMLTextAreaElement>(null);
  const textAreaRef = inputRef || internalTextAreaRef;
  const timeoutRefsRef = useRef<Set<ReturnType<typeof setTimeout>>>(new Set());

  useEffect(() => {
    setValue(initialValue);
    setDisplayValue(initialValue);
    setPastedImages([]);
    setHistoryIndex(-1);
    setIsInGlobalHistory(false);
    setHasUserTyped(false);
  }, [initialValue]);

  // Handle recipe prompt updates
  useEffect(() => {
    // If recipe is accepted and we have an initial prompt, and no messages yet, and we haven't set it before
    if (recipeAccepted && initialPrompt && messages.length === 0) {
      setDisplayValue(initialPrompt);
      setValue(initialPrompt);
      setTimeout(() => {
        textAreaRef.current?.focus();
      }, 0);
    }
  }, [recipeAccepted, initialPrompt, messages.length, textAreaRef]);

  const [isComposing, setIsComposing] = useState(false);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [savedInput, setSavedInput] = useState('');
  const [isInGlobalHistory, setIsInGlobalHistory] = useState(false);
  const [hasUserTyped, setHasUserTyped] = useState(false);

  // Use shared file drop hook for ChatInput
  const {
    droppedFiles: localDroppedFiles,
    setDroppedFiles: setLocalDroppedFiles,
    handleDrop: handleLocalDrop,
    handleDragOver: handleLocalDragOver,
  } = useFileDrop();

  // Merge local dropped files with parent dropped files
  const allDroppedFiles = useMemo(
    () => [...droppedFiles, ...localDroppedFiles],
    [droppedFiles, localDroppedFiles]
  );

  const handleRemoveDroppedFile = (idToRemove: string) => {
    // Remove from local dropped files
    setLocalDroppedFiles((prev) => prev.filter((file) => file.id !== idToRemove));

    // If it's from parent, call the parent's callback
    if (onFilesProcessed && droppedFiles.some((file) => file.id === idToRemove)) {
      onFilesProcessed();
    }
  };

  const handleRemovePastedImage = (idToRemove: string) => {
    setPastedImages((currentImages) => currentImages.filter((img) => img.id !== idToRemove));
  };

  useEffect(() => {
    if (textAreaRef.current) {
      textAreaRef.current.focus();
    }
  }, [textAreaRef]);

  // Load providers and get current model's token limit
  const loadProviderDetails = async () => {
    try {
      // Reset token limit loaded state
      setIsTokenLimitLoaded(false);

      // Use effective model/provider (includes overrides from in-session model changes),
      // fall back to config defaults
      let model = effectiveModel;
      let provider = effectiveProvider;
      if (!model || !provider) {
        const configModelAndProvider = await getCurrentModelAndProvider();
        model = configModelAndProvider.model;
        provider = configModelAndProvider.provider;
      }
      if (!model || !provider) {
        setIsTokenLimitLoaded(true);
        return;
      }

      // Priority 1: Check predefined models from environment
      const predefinedModels = getPredefinedModelsFromEnv();
      const predefinedModel = predefinedModels.find((m) => m.name === model);
      if (predefinedModel?.context_limit) {
        setTokenLimit(predefinedModel.context_limit);
        setIsTokenLimitLoaded(true);
        return;
      }

      // Priority 2: Check canonical model info (source of truth)
      const canonicalInfo = await fetchCanonicalModelInfo(provider, model);
      if (canonicalInfo?.context_limit) {
        setTokenLimit(canonicalInfo.context_limit);
        setIsTokenLimitLoaded(true);
        return;
      }

      // Priority 3: Fall back to provider metadata known_models (may be outdated)
      const providers = await getProviders(true);
      const currentProvider = providers.find((p) => p.name === provider);
      if (currentProvider?.metadata?.known_models) {
        const modelConfig = currentProvider.metadata.known_models.find((m) => m.name === model);
        if (modelConfig?.context_limit) {
          setTokenLimit(modelConfig.context_limit);
          setIsTokenLimitLoaded(true);
          return;
        }
      }

      // Priority 4: Use default if nothing else found
      setTokenLimit(TOKEN_LIMIT_DEFAULT);
      setIsTokenLimitLoaded(true);
    } catch (err) {
      console.error('Error loading providers or token limit:', err);
      // Set default limit on error
      setTokenLimit(TOKEN_LIMIT_DEFAULT);
      setIsTokenLimitLoaded(true);
    }
  };

  // Initial load and refresh when model changes (effective model includes overrides,
  // config model is the fallback for Hub/no-session contexts)
  useEffect(() => {
    loadProviderDetails();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [effectiveModel, effectiveProvider, configModel, configProvider]);

  // Handle tool count alerts and token usage
  useEffect(() => {
    clearAlerts();

    // Show alert when either there is registered token usage, or we know the limit
    if ((totalTokens && totalTokens > 0) || (isTokenLimitLoaded && tokenLimit)) {
      addAlert({
        type: AlertType.Info,
        message: intl.formatMessage(i18n.contextWindow),
        progress: {
          current: totalTokens || 0,
          total: tokenLimit,
        },
        showCompactButton: true,
        compactButtonDisabled: !totalTokens,
        onCompact: () => {
          window.dispatchEvent(new CustomEvent(AppEvents.HIDE_ALERT_POPOVER));
          handleSubmit({ msg: MANUAL_COMPACT_TRIGGER, images: [] });
        },
        compactIcon: <ScrollText size={12} />,
      });
    }

    // Add tool count alert if we have the data
    if (toolCount !== null && toolCount > TOOLS_MAX_SUGGESTED) {
      addAlert({
        type: AlertType.Warning,
        message: intl.formatMessage(i18n.tooManyTools, { toolCount, recommended: TOOLS_MAX_SUGGESTED }),
        action: {
          text: intl.formatMessage(i18n.viewExtensions),
          onClick: () => setView('extensions'),
        },
        autoShow: false, // Don't auto-show tool count warnings
      });
    }
    // We intentionally omit setView as it shouldn't trigger a re-render of alerts
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [totalTokens, toolCount, tokenLimit, isTokenLimitLoaded, addAlert, clearAlerts]);

  // Cleanup effect for component unmount - prevent memory leaks
  useEffect(() => {
    return () => {
      // Clear all tracked timeouts
      // eslint-disable-next-line react-hooks/exhaustive-deps
      const timeouts = timeoutRefsRef.current;
      timeouts.forEach((timeoutId) => {
        window.clearTimeout(timeoutId);
      });
      timeouts.clear();

      // Clear alerts to prevent memory leaks
      clearAlerts();
    };
  }, [clearAlerts]);

  const maxHeight = 10 * 24;

  // Immediate function to update actual value - no debounce for better responsiveness
  const updateValue = React.useCallback((value: string) => {
    setValue(value);
  }, []);

  const minTextareaHeight = 38;

  const debouncedAutosize = useMemo(
    () =>
      debounce((element: HTMLTextAreaElement) => {
        // Store current scroll position to prevent jump
        const scrollTop = element.scrollTop;

        // Temporarily set to auto to measure natural height, but use minHeight to prevent collapse
        element.style.height = `${minTextareaHeight}px`;
        const scrollHeight = element.scrollHeight;
        const newHeight = Math.max(minTextareaHeight, Math.min(scrollHeight, maxHeight));
        element.style.height = `${newHeight}px`;

        // Restore scroll position
        element.scrollTop = scrollTop;
      }, 50),
    [maxHeight, minTextareaHeight]
  );

  useEffect(() => {
    if (textAreaRef.current) {
      debouncedAutosize(textAreaRef.current);
    }
  }, [debouncedAutosize, displayValue, textAreaRef]);

  // Set consistent minimum height when displayValue is empty
  useEffect(() => {
    if (textAreaRef.current && displayValue === '') {
      textAreaRef.current.style.height = `${minTextareaHeight}px`;
    }
  }, [displayValue, textAreaRef, minTextareaHeight]);

  const handleChange = (evt: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = evt.target.value;
    const cursorPosition = evt.target.selectionStart;

    setDisplayValue(val);
    updateValue(val);
    setHasUserTyped(true);
    checkForMentionOrSlash(val, cursorPosition, evt.target);
  };

  const checkForMentionOrSlash = (
    text: string,
    cursorPosition: number,
    textArea: HTMLTextAreaElement
  ) => {
    const isSlashCommand = text.startsWith('/');
    const beforeCursor = text.slice(0, cursorPosition);
    const lastAtIndex = isSlashCommand ? 0 : beforeCursor.lastIndexOf('@');

    if (lastAtIndex === -1) {
      // No @ found, close mention popover
      setMentionPopover((prev) => ({ ...prev, isOpen: false }));
      return;
    }

    // Check if there's a space between @ and cursor (which would end the mention)
    const afterAt = beforeCursor.slice(lastAtIndex + 1);
    if (afterAt.includes(' ') || afterAt.includes('\n')) {
      setMentionPopover((prev) => ({ ...prev, isOpen: false }));
      return;
    }

    // Calculate position for the popover - position it above the chat input
    const textAreaRect = textArea.getBoundingClientRect();

    setMentionPopover((prev) => ({
      ...prev,
      isOpen: true,
      position: {
        x: textAreaRect.left,
        y: textAreaRect.top, // Position at the top of the textarea
      },
      query: afterAt,
      mentionStart: lastAtIndex,
      selectedIndex: 0, // Reset selection when query changes
      isSlashCommand,
      // filteredFiles will be populated by the MentionPopover component
    }));
  };

  const convertImagesToImageData = useCallback((): ImageData[] => {
    const pastedImageData: ImageData[] = pastedImages
      .filter((img) => img.dataUrl && !img.error && !img.isLoading)
      .map((img) => {
        const matches = img.dataUrl.match(/^data:([^;]+);base64,(.+)$/);
        if (matches) {
          return {
            data: matches[2],
            mimeType: matches[1],
          };
        }
        return null;
      })
      .filter((img): img is ImageData => img !== null);

    const droppedImageData: ImageData[] = allDroppedFiles
      .filter((file) => file.isImage && file.dataUrl && !file.error && !file.isLoading)
      .map((file) => {
        const matches = file.dataUrl!.match(/^data:([^;]+);base64,(.+)$/);
        if (matches) {
          return {
            data: matches[2],
            mimeType: matches[1],
          };
        }
        return null;
      })
      .filter((img): img is ImageData => img !== null);

    return [...pastedImageData, ...droppedImageData];
  }, [pastedImages, allDroppedFiles]);

  const appendDroppedFilePaths = useCallback(
    (text: string): string => {
      const droppedFilePaths = allDroppedFiles
        .filter((file) => !file.isImage && !file.error && !file.isLoading)
        .map((file) => file.path);

      if (droppedFilePaths.length > 0) {
        const pathsString = droppedFilePaths.join(' ');
        return text ? `${text} ${pathsString}` : pathsString;
      }
      return text;
    },
    [allDroppedFiles]
  );

  const clearInputState = useCallback(() => {
    setDisplayValue('');
    setValue('');
    setPastedImages([]);
    if (onFilesProcessed && droppedFiles.length > 0) {
      onFilesProcessed();
    }
    if (localDroppedFiles.length > 0) {
      setLocalDroppedFiles([]);
    }
  }, [droppedFiles.length, localDroppedFiles.length, onFilesProcessed, setLocalDroppedFiles]);

  const handlePaste = async (evt: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const files = Array.from(evt.clipboardData.files || []);
    const imageFiles = files.filter((file) => file.type.startsWith('image/'));

    if (imageFiles.length === 0) return;

    // Check if adding these images would exceed the limit
    if (pastedImages.length + imageFiles.length > MAX_IMAGES_PER_MESSAGE) {
      // Show error message to user
      setPastedImages((prev) => [
        ...prev,
        {
          id: `error-${Date.now()}`,
          dataUrl: '',
          isLoading: false,
          error: `Cannot paste ${imageFiles.length} image(s). Maximum ${MAX_IMAGES_PER_MESSAGE} images per message allowed. Currently have ${pastedImages.length}.`,
        },
      ]);

      // Remove the error message after 5 seconds with cleanup tracking
      const timeoutId = setTimeout(() => {
        setPastedImages((prev) => prev.filter((img) => !img.id.startsWith('error-')));
        timeoutRefsRef.current.delete(timeoutId);
      }, 5000);
      timeoutRefsRef.current.add(timeoutId);

      return;
    }

    evt.preventDefault();

    // Process each image file
    const newImages: PastedImage[] = [];

    for (const file of imageFiles) {
      const imageId = `img-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;

      // Add the image with loading state
      newImages.push({
        id: imageId,
        dataUrl: '',
        isLoading: true,
      });

      // Process the image asynchronously
      const reader = new FileReader();
      reader.onload = async (e) => {
        const dataUrl = e.target?.result as string;
        if (dataUrl) {
          const compressedDataUrl = await compressImageDataUrl(dataUrl);
          setPastedImages((prev) =>
            prev.map((img) =>
              img.id === imageId ? { ...img, dataUrl: compressedDataUrl, isLoading: false } : img
            )
          );
        }
      };
      reader.onerror = () => {
        console.error('Failed to read image file:', file.name);
        setPastedImages((prev) =>
          prev.map((img) =>
            img.id === imageId
              ? { ...img, error: intl.formatMessage(i18n.failedToReadImage), isLoading: false }
              : img
          )
        );
      };
      reader.readAsDataURL(file);
    }

    // Add all new images to the existing list
    setPastedImages((prev) => [...prev, ...newImages]);
  };

  // Cleanup debounced functions on unmount
  useEffect(() => {
    return () => {
      debouncedAutosize.cancel?.();
    };
  }, [debouncedAutosize]);

  // Handlers for composition events, which are crucial for proper IME behavior
  const handleCompositionStart = () => {
    setIsComposing(true);
  };

  const handleCompositionEnd = () => {
    setIsComposing(false);
  };

  const handleHistoryNavigation = (evt: React.KeyboardEvent<HTMLTextAreaElement>) => {
    const isUp = evt.key === 'ArrowUp';
    const isDown = evt.key === 'ArrowDown';

    // Only handle up/down keys with Cmd/Ctrl modifier
    if ((!isUp && !isDown) || !(evt.metaKey || evt.ctrlKey) || evt.altKey || evt.shiftKey) {
      return;
    }

    // Only prevent history navigation if the user has actively typed something
    // This allows history navigation when text is populated from history or other sources
    // but prevents it when the user is actively editing text
    if (hasUserTyped && displayValue.trim() !== '') {
      return;
    }

    evt.preventDefault();

    // Get global history once to avoid multiple calls
    const globalHistory = LocalMessageStorage.getRecentMessages() || [];

    // Save current input if we're just starting to navigate history
    if (historyIndex === -1) {
      setSavedInput(displayValue || '');
      setIsInGlobalHistory(commandHistory.length === 0);
    }

    // Determine which history we're using
    const currentHistory = isInGlobalHistory ? globalHistory : commandHistory;
    let newIndex = historyIndex;
    let newValue = '';

    // Handle navigation
    if (isUp) {
      // Moving up through history
      if (newIndex < currentHistory.length - 1) {
        // Still have items in current history
        newIndex = historyIndex + 1;
        newValue = currentHistory[newIndex];
      } else if (!isInGlobalHistory && globalHistory.length > 0) {
        // Switch to global history
        setIsInGlobalHistory(true);
        newIndex = 0;
        newValue = globalHistory[newIndex];
      }
    } else {
      // Moving down through history
      if (newIndex > 0) {
        // Still have items in current history
        newIndex = historyIndex - 1;
        newValue = currentHistory[newIndex];
      } else if (isInGlobalHistory && commandHistory.length > 0) {
        // Switch to chat history
        setIsInGlobalHistory(false);
        newIndex = commandHistory.length - 1;
        newValue = commandHistory[newIndex];
      } else {
        // Return to original input
        newIndex = -1;
        newValue = savedInput;
      }
    }

    // Update display if we have a new value
    if (newIndex !== historyIndex) {
      setHistoryIndex(newIndex);
      if (newIndex === -1) {
        setDisplayValue(savedInput || '');
        setValue(savedInput || '');
      } else {
        setDisplayValue(newValue || '');
        setValue(newValue || '');
      }
      // Reset hasUserTyped when we populate from history
      setHasUserTyped(false);
    }
  };

  const handleInterruptionAndQueue = () => {
    if (!isLoading || !hasSubmittableContent) {
      return false;
    }

    const imageData = convertImagesToImageData();
    const contentToQueue = appendDroppedFilePaths(displayValue.trim());

    const interruptionMatch = detectInterruption(displayValue.trim());

    if (interruptionMatch && interruptionMatch.shouldInterrupt) {
      setLastInterruption(interruptionMatch.matchedText);
      if (onStop) onStop();
      queuePausedRef.current = true;

      // For interruptions, we need to queue the message to be sent after the stop completes
      // rather than trying to send it immediately while the system is still loading
      const interruptionMessage: QueuedMessage = {
        id: Date.now().toString() + Math.random().toString(36).substr(2, 9),
        content: contentToQueue,
        timestamp: Date.now(),
        images: imageData,
      };

      // Add the interruption message to the front of the queue so it gets sent first
      setQueuedMessages((prev) => [interruptionMessage, ...prev]);

      clearInputState();
      return true;
    }

    const newMessage: QueuedMessage = {
      id: Date.now().toString() + Math.random().toString(36).substr(2, 9),
      content: contentToQueue,
      timestamp: Date.now(),
      images: imageData,
    };
    setQueuedMessages((prev) => {
      const newQueue = [...prev, newMessage];
      // If adding to an empty queue, reset the paused state
      if (prev.length === 0) {
        queuePausedRef.current = false;
        setLastInterruption(null);
      }
      return newQueue;
    });
    clearInputState();
    return true;
  };

  const canSubmit =
    !isLoading &&
    (displayValue.trim() ||
      pastedImages.some((img) => img.dataUrl && !img.error && !img.isLoading) ||
      allDroppedFiles.some((file) => !file.error && !file.isLoading));

  const performSubmit = useCallback(
    (text?: string) => {
      const imageData = convertImagesToImageData();
      const textToSend = appendDroppedFilePaths(text ?? displayValue.trim());

      if (textToSend || imageData.length > 0) {
        // Store original message in history
        if (displayValue.trim()) {
          LocalMessageStorage.addMessage(displayValue);
        } else {
          const droppedFilePaths = allDroppedFiles
            .filter((file) => !file.isImage && !file.error && !file.isLoading)
            .map((file) => file.path);
          if (droppedFilePaths.length > 0) {
            LocalMessageStorage.addMessage(droppedFilePaths.join(' '));
          }
        }

        handleSubmit({ msg: textToSend, images: imageData });

        // Auto-resume queue after sending a NON-interruption message (if it was paused due to interruption)
        if (
          queuePausedRef.current &&
          lastInterruption &&
          textToSend &&
          !detectInterruption(textToSend)
        ) {
          queuePausedRef.current = false;
          setLastInterruption(null);
        }

        clearInputState();
        setHistoryIndex(-1);
        setSavedInput('');
        setIsInGlobalHistory(false);
        setHasUserTyped(false);
      }
    },
    [
      convertImagesToImageData,
      appendDroppedFilePaths,
      displayValue,
      allDroppedFiles,
      handleSubmit,
      lastInterruption,
      clearInputState,
    ]
  );

  const handleKeyDown = (evt: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (mentionPopover.isOpen && mentionPopoverRef.current) {
      if (evt.key === 'ArrowDown') {
        evt.preventDefault();
        const displayFiles = mentionPopoverRef.current.getDisplayFiles();
        const maxIndex = Math.max(0, displayFiles.length - 1);
        setMentionPopover((prev) => ({
          ...prev,
          selectedIndex: Math.min(prev.selectedIndex + 1, maxIndex),
        }));
        return;
      }
      if (evt.key === 'ArrowUp') {
        evt.preventDefault();
        setMentionPopover((prev) => ({
          ...prev,
          selectedIndex: Math.max(prev.selectedIndex - 1, 0),
        }));
        return;
      }
      if (evt.key === 'Enter') {
        evt.preventDefault();
        mentionPopoverRef.current.selectFile(mentionPopover.selectedIndex);
        return;
      }
      if (evt.key === 'Escape') {
        evt.preventDefault();
        setMentionPopover((prev) => ({ ...prev, isOpen: false }));
        return;
      }
    }

    handleHistoryNavigation(evt);

    if (evt.key === 'Enter') {
      // should not trigger submit on Enter if it's composing (IME input in progress) or shift/alt(option) is pressed
      if (evt.shiftKey || isComposing) {
        // Allow line break for Shift+Enter, or during IME composition
        return;
      }

      if (evt.altKey) {
        const newValue = displayValue + '\n';
        setDisplayValue(newValue);
        setValue(newValue);
        return;
      }

      evt.preventDefault();

      // Handle interruption and queue logic
      if (handleInterruptionAndQueue()) {
        return;
      }

      if (canSubmit) {
        performSubmit();
      }
    }
  };

  const onFormSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (isLoading && hasSubmittableContent) {
      handleInterruptionAndQueue();
      return;
    }
    const canSubmit =
      !isLoading &&
      (displayValue.trim() ||
        pastedImages.some((img) => img.dataUrl && !img.error && !img.isLoading) ||
        allDroppedFiles.some((file) => !file.error && !file.isLoading));
    if (canSubmit) {
      performSubmit();
    }
  };

  const fileInputRef = React.useRef<HTMLInputElement>(null);

  const handleFileSelect = () => {
    if (isFilePickerOpen) return;
    fileInputRef.current?.click();
  };

  const handleFileInputChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files || files.length === 0) return;

    setIsFilePickerOpen(true);
    const file = files[0];
    const isImage = file.type.startsWith('image/');

    if (isImage) {
      trackFileAttached('file');

      if (pastedImages.length >= MAX_IMAGES_PER_MESSAGE) {
        console.warn(`Maximum ${MAX_IMAGES_PER_MESSAGE} images per message`);
        setIsFilePickerOpen(false);
        return;
      }

      const uniqueId = `upload-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

      setPastedImages((prev) => [
        ...prev,
        {
          id: uniqueId,
          dataUrl: '',
          isLoading: true,
          error: undefined,
        },
      ]);

      const reader = new FileReader();
      reader.onload = async (evt) => {
        const dataUrl = evt.target?.result as string;
        if (dataUrl) {
          const compressedDataUrl = await compressImageDataUrl(dataUrl);
          setPastedImages((prev) =>
            prev.map((img) =>
              img.id === uniqueId
                ? { ...img, dataUrl: compressedDataUrl, isLoading: false, error: undefined }
                : img
            )
          );
        }
      };
      reader.onerror = () => {
        setPastedImages((prev) =>
          prev.map((img) =>
            img.id === uniqueId
              ? { ...img, isLoading: false, error: intl.formatMessage(i18n.failedToReadImage) }
              : img
          )
        );
      };
      reader.readAsDataURL(file);
    } else {
      trackFileAttached('file');
      const path = window.electron.getPathForFile(file);
      const newValue = displayValue.trim() ? `${displayValue.trim()} ${path}` : path;
      setDisplayValue(newValue);
      setValue(newValue);
    }

    textAreaRef.current?.focus();
    setIsFilePickerOpen(false);
    if (e.target) {
      e.target.value = '';
    }
  };

  const handleMentionItemSelect = (itemText: string) => {
    // Replace the @ mention with the file path
    const beforeMention = displayValue.slice(0, mentionPopover.mentionStart);
    const afterMention = displayValue.slice(
      mentionPopover.mentionStart + 1 + mentionPopover.query.length
    );
    const newValue = `${beforeMention}${itemText}${afterMention}`;

    setDisplayValue(newValue);
    setValue(newValue);
    setMentionPopover((prev) => ({ ...prev, isOpen: false }));
    textAreaRef.current?.focus();

    // Set cursor position after the inserted file path
    setTimeout(() => {
      if (textAreaRef.current) {
        const newCursorPosition = beforeMention.length + itemText.length;
        textAreaRef.current.setSelectionRange(newCursorPosition, newCursorPosition);
      }
    }, 0);
  };

  const hasSubmittableContent =
    displayValue.trim() ||
    pastedImages.some((img) => img.dataUrl && !img.error && !img.isLoading) ||
    allDroppedFiles.some((file) => !file.error && !file.isLoading);
  const isAnyImageLoading = pastedImages.some((img) => img.isLoading);
  const isAnyDroppedFileLoading = allDroppedFiles.some((file) => file.isLoading);

  const isSubmitButtonDisabled =
    !hasSubmittableContent ||
    isAnyImageLoading ||
    isAnyDroppedFileLoading ||
    isRecording ||
    isTranscribing ||
    chatState === ChatState.RestartingAgent;

  const getSubmitButtonTooltip = (): string => {
    if (isAnyImageLoading) return intl.formatMessage(i18n.waitingForImages);
    if (isAnyDroppedFileLoading) return intl.formatMessage(i18n.processingDroppedFiles);
    if (isRecording) return intl.formatMessage(i18n.recording);
    if (isTranscribing) return intl.formatMessage(i18n.transcribing);
    if (chatState === ChatState.RestartingAgent) return intl.formatMessage(i18n.restartingSession);
    if (!hasSubmittableContent) return intl.formatMessage(i18n.typeMessage);
    return intl.formatMessage(i18n.send);
  };

  // Queue management functions - no storage persistence, only in-memory
  const handleRemoveQueuedMessage = (messageId: string) => {
    setQueuedMessages((prev) => prev.filter((msg) => msg.id !== messageId));
  };

  const handleClearQueue = () => {
    setQueuedMessages([]);
    queuePausedRef.current = false;
    setLastInterruption(null);
  };

  const handleReorderMessages = (reorderedMessages: QueuedMessage[]) => {
    setQueuedMessages(reorderedMessages);
  };

  const handleEditMessage = (messageId: string, newContent: string) => {
    setQueuedMessages((prev) =>
      prev.map((msg) => (msg.id === messageId ? { ...msg, content: newContent } : msg))
    );
  };

  const handleStopAndSend = (messageId: string) => {
    const messageToSend = queuedMessages.find((msg) => msg.id === messageId);
    if (!messageToSend) return;

    // Stop current processing and temporarily pause queue to prevent double-send
    if (onStop) onStop();
    const wasPaused = queuePausedRef.current;
    queuePausedRef.current = true;

    // Remove the message from queue and send it immediately
    setQueuedMessages((prev) => prev.filter((msg) => msg.id !== messageId));
    LocalMessageStorage.addMessage(messageToSend.content);
    handleSubmit({ msg: messageToSend.content, images: messageToSend.images });

    // Restore previous pause state after a brief delay to prevent race condition
    setTimeout(() => {
      queuePausedRef.current = wasPaused;
    }, 100);
  };

  const handleResumeQueue = () => {
    queuePausedRef.current = false;
    setLastInterruption(null);
    if (!isLoading && queuedMessages.length > 0) {
      const nextMessage = queuedMessages[0];
      LocalMessageStorage.addMessage(nextMessage.content);
      handleSubmit({ msg: nextMessage.content, images: nextMessage.images });
      setQueuedMessages((prev) => {
        const newQueue = prev.slice(1);
        // If queue becomes empty after processing, clear the paused state
        if (newQueue.length === 0) {
          queuePausedRef.current = false;
          setLastInterruption(null);
        }
        return newQueue;
      });
    }
  };

  return (
    <div
      className={`flex flex-col relative h-auto p-4 transition-colors ${
        disableAnimation ? '' : 'page-transition'
      } ${
        isFocused
          ? 'border-border-secondary hover:border-border-secondary'
          : 'border-border-primary hover:border-border-primary'
      } bg-background-primary z-10 rounded-t-2xl`}
      data-drop-zone="true"
      onDrop={handleLocalDrop}
      onDragOver={handleLocalDragOver}
    >
      <input
        ref={fileInputRef}
        type="file"
        onChange={handleFileInputChange}
        style={{ display: 'none' }}
        accept="*/*"
      />
      {/* Message Queue Display */}
      {queuedMessages.length > 0 && (
        <MessageQueue
          queuedMessages={queuedMessages}
          onRemoveMessage={handleRemoveQueuedMessage}
          onClearQueue={handleClearQueue}
          onStopAndSend={handleStopAndSend}
          onReorderMessages={handleReorderMessages}
          onEditMessage={handleEditMessage}
          onTriggerQueueProcessing={handleResumeQueue}
          editingMessageIdRef={editingMessageIdRef}
          isPaused={queuePausedRef.current}
          className="border-b border-border-primary"
        />
      )}
      {/* Input row with inline action buttons wrapped in form */}
      <form onSubmit={onFormSubmit} className="relative">
        <div className="relative">
          <textarea
            data-testid="chat-input"
            autoFocus
            id="dynamic-textarea"
            placeholder={isRecording ? '' : getNavigationShortcutText()}
            value={displayValue}
            onChange={handleChange}
            onCompositionStart={handleCompositionStart}
            onCompositionEnd={handleCompositionEnd}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            onFocus={() => setIsFocused(true)}
            onBlur={() => setIsFocused(false)}
            ref={textAreaRef}
            rows={1}
            readOnly={isRecording}
            style={{
              minHeight: `${minTextareaHeight}px`,
              maxHeight: `${maxHeight}px`,
              overflowY: 'auto',
              paddingRight: dictationProvider ? '180px' : '120px',
            }}
            className="w-full outline-none border-none focus:ring-0 bg-transparent px-3 pt-3 pb-1.5 text-sm resize-none text-text-primary placeholder:text-text-secondary"
          />

          {/* Inline action buttons - absolutely positioned on the right */}
          <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-1">
            {/* Microphone button - show only if provider is selected */}
            {dictationProvider && (
              <>
                {!isEnabled ? (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <span className="inline-flex">
                        <Button
                          type="button"
                          size="sm"
                          shape="round"
                          variant="outline"
                          onClick={() => {}}
                          disabled={true}
                          className="bg-slate-600 text-white cursor-not-allowed opacity-50 border-slate-600 rounded-full px-6 py-2"
                        >
                          <Microphone />
                        </Button>
                      </span>
                    </TooltipTrigger>
                    <TooltipContent>
                      {dictationProvider === 'openai' ? (
                        <p>
                          OpenAI API key is not configured. Set it up in <b>Settings</b> {'>'}{' '}
                          <b>Models.</b>
                        </p>
                      ) : dictationProvider === 'elevenlabs' ? (
                        <p>
                          ElevenLabs API key is not configured. Set it up in <b>Settings</b> {'>'}{' '}
                          <b>Chat</b> {'>'} <b>Voice Dictation.</b>
                        </p>
                      ) : dictationProvider === 'local' ? (
                        <p>
                          Local Whisper model not found. Download a model in{' '}
                          <b>Settings &gt; Dictation &gt; Local (Offline)</b>
                        </p>
                      ) : (
                        <p>Dictation provider is not properly configured.</p>
                      )}
                    </TooltipContent>
                  </Tooltip>
                ) : (
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        type="button"
                        size="sm"
                        shape="round"
                        variant="outline"
                        onClick={() => {
                          if (isRecording) {
                            trackVoiceDictation('stop');
                            stopRecording();
                          } else {
                            trackVoiceDictation('start');
                            startRecording();
                          }
                        }}
                        disabled={isTranscribing}
                        className={`rounded-full px-6 py-2 ${
                          isRecording
                            ? 'bg-red-500 text-white hover:bg-red-600 border-red-500'
                            : isTranscribing
                              ? 'bg-slate-600 text-white cursor-not-allowed animate-pulse border-slate-600'
                              : 'bg-slate-600 text-white hover:bg-slate-700 border-slate-600'
                        }`}
                      >
                        <Microphone />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>
                      <p>
                        Voice dictation
                        {isRecording ? '' : ' • Say "submit" to send'}
                      </p>
                    </TooltipContent>
                  </Tooltip>
                )}
              </>
            )}

            {/* Send/Stop button */}
            {isLoading && !hasSubmittableContent ? (
              <Button
                type="button"
                onClick={onStop}
                size="sm"
                shape="round"
                variant="outline"
                className="bg-slate-600 text-white hover:bg-slate-700 border-slate-600 rounded-full px-6 py-2"
              >
                <Stop />
              </Button>
            ) : (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span>
                    <Button
                      type="submit"
                      size="sm"
                      shape="round"
                      variant="outline"
                      disabled={isSubmitButtonDisabled}
                      className={`rounded-full px-10 py-2 flex items-center gap-2 ${
                        isSubmitButtonDisabled
                          ? 'bg-slate-600 text-white cursor-not-allowed opacity-50 border-slate-600'
                          : 'bg-slate-600 text-white hover:bg-slate-700 border-slate-600 hover:cursor-pointer'
                      }`}
                    >
                      <Send className="w-4 h-4" />
                      <span className="text-sm">Send</span>
                    </Button>
                  </span>
                </TooltipTrigger>
                <TooltipContent>
                  <p>{getSubmitButtonTooltip()}</p>
                </TooltipContent>
              </Tooltip>
            )}

            {/* Recording/transcribing status indicator - positioned above the button row */}
            {(isRecording || isTranscribing) && (
              <div className="absolute right-0 -top-8 bg-background-primary px-2 py-1 rounded text-xs whitespace-nowrap shadow-md border border-border-primary">
                <span className="flex items-center gap-2">
                  {isRecording && (
                    <span className="flex items-center gap-1 text-text-secondary">
                      <span className="inline-block w-2 h-2 bg-red-500 rounded-full animate-pulse" />
                      Listening
                    </span>
                  )}
                  {isRecording && isTranscribing && <span className="text-text-secondary">•</span>}
                  {isTranscribing && (
                    <span className="flex items-center gap-1 text-blue-500">
                      <span className="inline-block w-2 h-2 bg-blue-500 rounded-full animate-pulse" />
                      Transcribing
                    </span>
                  )}
                </span>
              </div>
            )}
          </div>
        </div>
      </form>

      {/* Combined files and images preview */}
      {(pastedImages.length > 0 || allDroppedFiles.length > 0) && (
        <div className="flex flex-wrap gap-2 p-4 mt-2 border-t border-border-primary">
          {/* Render pasted images first */}
          {pastedImages.map((img) => (
            <div key={img.id} className="relative group w-20 h-20">
              {img.dataUrl && (
                <img
                  src={img.dataUrl}
                  alt={`Pasted image ${img.id}`}
                  className={`w-full h-full object-cover rounded border ${img.error ? 'border-red-500' : 'border-border-primary'}`}
                />
              )}
              {img.isLoading && (
                <div className="absolute inset-0 flex items-center justify-center bg-black bg-opacity-50 rounded">
                  <div className="animate-spin rounded-full h-6 w-6 border-t-2 border-b-2 border-white"></div>
                </div>
              )}
              {img.error && !img.isLoading && (
                <div className="absolute inset-0 flex flex-col items-center justify-center bg-black bg-opacity-75 rounded p-1 text-center">
                  <p className="text-red-400 text-[10px] leading-tight break-all">
                    {img.error.substring(0, 50)}
                  </p>
                </div>
              )}
              {!img.isLoading && (
                <Button
                  type="button"
                  shape="round"
                  onClick={() => handleRemovePastedImage(img.id)}
                  className="absolute -top-1 -right-1 opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity z-10"
                  aria-label={intl.formatMessage(i18n.removeImage)}
                  variant="outline"
                  size="xs"
                >
                  <Close />
                </Button>
              )}
            </div>
          ))}

          {/* Render dropped files after pasted images */}
          {allDroppedFiles.map((file) => (
            <div key={file.id} className="relative group">
              {file.isImage ? (
                // Image preview
                <div className="w-20 h-20">
                  {file.dataUrl && (
                    <img
                      src={file.dataUrl}
                      alt={file.name}
                      className={`w-full h-full object-cover rounded border ${file.error ? 'border-red-500' : 'border-border-primary'}`}
                    />
                  )}
                  {file.isLoading && (
                    <div className="absolute inset-0 flex items-center justify-center bg-black bg-opacity-50 rounded">
                      <div className="animate-spin rounded-full h-6 w-6 border-t-2 border-b-2 border-white"></div>
                    </div>
                  )}
                  {file.error && !file.isLoading && (
                    <div className="absolute inset-0 flex flex-col items-center justify-center bg-black bg-opacity-75 rounded p-1 text-center">
                      <p className="text-red-400 text-[10px] leading-tight break-all">
                        {file.error.substring(0, 30)}
                      </p>
                    </div>
                  )}
                </div>
              ) : (
                // File box preview
                <div className="flex items-center gap-2 px-3 py-2 bg-bgSubtle border border-border-primary rounded-lg min-w-[120px] max-w-[200px]">
                  <div className="flex-shrink-0 w-8 h-8 bg-background-primary border border-border-primary rounded flex items-center justify-center text-xs font-mono text-text-secondary">
                    {file.name.split('.').pop()?.toUpperCase() || 'FILE'}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-sm text-text-primary truncate" title={file.name}>
                      {file.name}
                    </p>
                    <p className="text-xs text-text-secondary">{file.type || intl.formatMessage(i18n.unknownType)}</p>
                  </div>
                </div>
              )}
              {!file.isLoading && (
                <Button
                  type="button"
                  shape="round"
                  onClick={() => handleRemoveDroppedFile(file.id)}
                  className="absolute -top-1 -right-1 opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity z-10"
                  aria-label={intl.formatMessage(i18n.removeFile)}
                  variant="outline"
                  size="xs"
                >
                  <Close />
                </Button>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Secondary actions and controls row below input */}
      <div className="flex flex-row items-center gap-1 p-2 relative">
        <DirSwitcher
          className="mr-0"
          sessionId={sessionId ?? undefined}
          workingDir={sessionWorkingDir ?? getInitialWorkingDir()}
          onWorkingDirChange={(newDir) => {
            setSessionWorkingDir(newDir);
            if (onWorkingDirChange) {
              onWorkingDirChange(newDir);
            }
          }}
          onRestartStart={() => setChatState?.(ChatState.RestartingAgent)}
          onRestartEnd={() => setChatState?.(ChatState.Idle)}
        />
        <div className="w-px h-4 bg-border-primary mx-2" />
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              type="button"
              onClick={handleFileSelect}
              disabled={isFilePickerOpen}
              variant="ghost"
              size="sm"
              className={`flex items-center justify-center text-text-primary/70 hover:text-text-primary text-xs transition-colors ${isFilePickerOpen ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`}
            >
              <Attach className="w-4 h-4" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Attach file</TooltipContent>
        </Tooltip>
        <div className="w-px h-4 bg-border-primary mx-2" />
        {/* Model selector, mode selector, alerts, summarize button */}
        <div className="flex flex-row items-center">
          {/* Cost Tracker */}
          {COST_TRACKING_ENABLED && (
            <>
              <div className="flex items-center h-full ml-1 mr-1">
                <CostTracker
                  inputTokens={accumulatedInputTokens}
                  outputTokens={accumulatedOutputTokens}
                  sessionCosts={sessionCosts}
                  model={effectiveModel}
                  provider={effectiveProvider}
                />
              </div>
            </>
          )}
          <ContextWindowIndicator
            totalTokens={totalTokens || 0}
            tokenLimit={tokenLimit}
            alerts={alerts}
          />
          <Tooltip>
            <div>
              <ModelsBottomBar
                sessionId={sessionId}
                dropdownRef={dropdownRef}
                setView={setView}
                sessionModel={effectiveModel}
                sessionProvider={effectiveProvider}
                onModelChanged={setModelOverride}
                sessionLoaded={sessionLoaded}
              />
            </div>
          </Tooltip>
          <div className="w-px h-4 bg-border-primary mx-2" />
          <BottomMenuModeSelection sessionId={sessionId} />
          <div className="w-px h-4 bg-border-primary mx-2" />
          <BottomMenuExtensionSelection sessionId={sessionId} />
          {sessionId && messages.length > 0 && (
            <>
              <div className="w-px h-4 bg-border-primary mx-2" />
              <div className="flex items-center h-full">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      onClick={() => {
                        if (recipe) {
                          trackEditRecipeOpened();
                          setShowEditRecipeModal(true);
                        } else {
                          trackCreateRecipeOpened();
                          setShowCreateRecipeModal(true);
                        }
                      }}
                      variant="ghost"
                      size="sm"
                      className="flex items-center justify-center text-text-primary/70 hover:text-text-primary text-xs cursor-pointer"
                    >
                      <ChefHat size={16} />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    {recipe ? intl.formatMessage(i18n.viewEditRecipe) : intl.formatMessage(i18n.createRecipeFromSession)}
                  </TooltipContent>
                </Tooltip>
              </div>
            </>
          )}
          {sessionId && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  onClick={() => {
                    trackDiagnosticsOpened();
                    setDiagnosticsOpen(true);
                  }}
                  variant="ghost"
                  size="sm"
                  className="flex items-center justify-center text-text-primary/70 hover:text-text-primary text-xs cursor-pointer transition-colors"
                >
                  <Bug className="w-4 h-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Generate diagnostics bundle</TooltipContent>
            </Tooltip>
          )}
        </div>
        {sessionId && diagnosticsOpen && (
          <DiagnosticsModal
            isOpen={diagnosticsOpen}
            onClose={() => setDiagnosticsOpen(false)}
            sessionId={sessionId}
          />
        )}
        <MentionPopover
          ref={mentionPopoverRef}
          isOpen={mentionPopover.isOpen}
          isSlashCommand={mentionPopover.isSlashCommand}
          onClose={() => setMentionPopover((prev) => ({ ...prev, isOpen: false }))}
          onSelect={handleMentionItemSelect}
          position={mentionPopover.position}
          query={mentionPopover.query}
          selectedIndex={mentionPopover.selectedIndex}
          onSelectedIndexChange={(index) =>
            setMentionPopover((prev) => ({ ...prev, selectedIndex: index }))
          }
          workingDir={sessionWorkingDir ?? getInitialWorkingDir()}
        />

        {sessionId && showCreateRecipeModal && (
          <CreateRecipeFromSessionModal
            isOpen={showCreateRecipeModal}
            onClose={() => setShowCreateRecipeModal(false)}
            sessionId={sessionId}
          />
        )}

        {recipe && showEditRecipeModal && (
          <CreateEditRecipeModal
            isOpen={showEditRecipeModal}
            onClose={() => setShowEditRecipeModal(false)}
            recipe={recipe}
            recipeId={recipeId}
          />
        )}
      </div>
    </div>
  );
}
