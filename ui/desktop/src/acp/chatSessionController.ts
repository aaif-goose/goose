import { v7 as uuidv7 } from 'uuid';
import type { GooseExtension } from '@aaif/goose-acp-client';
import { AppEvents } from '../constants/events';
import { ChatState } from '../types/chatState';
import type { Session } from '../types/session';
import { showExtensionLoadResults } from '../utils/extensionErrorUtils';
import {
  createUserMessage,
  getPendingToolConfirmationIds,
  type ImageData,
  type Message,
} from '../types/message';
import {
  acpChatSessionActions,
  acpChatSessionStore,
  type AcpChatSessionSnapshot,
} from './chatSessionStore';
import { cancelAcpElicitationRequestsForSession } from './elicitationRequests';
import {
  formatAcpError,
  parseAcpCreditsExhaustedError,
  type AcpCreditsExhaustedError,
} from './errors';
import { cancelAcpPermissionRequestsForSession } from './permissionRequests';
import { acpCancelPrompt, acpPromptSession } from './prompt';
import {
  acpForkSession,
  acpLoadSession,
  acpNewSession,
  acpTruncateSessionConversation,
  isAcpSessionLoadInFlight,
  sessionInfoToSession,
  type AcpRecipeOptions,
} from './sessions';

export interface AcpLoadSessionOptions {
  onSessionLoaded?: () => void;
}

export interface AcpSnapshotOptions {
  getCurrentSnapshot(): AcpChatSessionSnapshot | undefined;
}

export interface AcpSubmitMessageOptions extends AcpSnapshotOptions {
  onFinish(error?: string): void | Promise<void>;
}

export interface AcpChatSessionController {
  createSession(
    cwd: string,
    gooseExtensions: GooseExtension[] | undefined,
    recipe?: AcpRecipeOptions
  ): Promise<Session>;
  loadSession(sessionId: string, options?: AcpLoadSessionOptions): Promise<void>;
  loadFullSessionHistory(sessionId: string): Promise<void>;
  restoreSession(sessionId: string): Promise<void>;
  submitMessage(
    sessionId: string,
    userMessage: Message,
    options: AcpSubmitMessageOptions
  ): Promise<void>;
  stop(sessionId: string): void;
  updateMessage(
    sessionId: string,
    messageId: string,
    newContent: string,
    editType: 'fork' | 'edit',
    retainedImages: ImageData[],
    options: AcpSubmitMessageOptions
  ): Promise<void>;
}

function createAcpCreditsExhaustedMessage(error: AcpCreditsExhaustedError): Message {
  return {
    id: uuidv7(),
    role: 'assistant',
    created: Math.floor(Date.now() / 1000),
    content: [
      {
        type: 'systemNotification',
        notificationType: 'creditsExhausted',
        msg: error.message,
        ...(error.url ? { data: { top_up_url: error.url } } : {}),
      },
    ],
    metadata: { userVisible: true, agentVisible: false },
  };
}

function assertNoPendingPromptCancellation(sessionId: string): void {
  const snapshot = acpChatSessionStore.getSnapshot(sessionId);
  if (snapshot?.pendingCancelPromptAttemptId) {
    throw new Error('Cannot submit while prompt cancellation is pending');
  }
}

async function forkSessionWithEditedMessage(
  sessionId: string,
  message: Message,
  editedMessage: string,
  editedImages: ImageData[]
): Promise<void> {
  const targetSessionId = await acpForkSession(sessionId, message.created);

  const event = new CustomEvent(AppEvents.SESSION_FORKED, {
    detail: {
      newSessionId: targetSessionId,
      shouldStartAgent: true,
      editedMessage,
      editedImages,
    },
  });
  window.dispatchEvent(event);
}

async function createSession(
  cwd: string,
  gooseExtensions: GooseExtension[] | undefined,
  recipe?: AcpRecipeOptions
): Promise<Session> {
  const { sessionId, sessionInfo, meta } = await acpNewSession(cwd, gooseExtensions, recipe);
  const session = sessionInfoToSession(sessionInfo, meta);

  showExtensionLoadResults(meta.extensionResults);
  window.dispatchEvent(
    new CustomEvent(AppEvents.SESSION_EXTENSIONS_LOADED, { detail: { sessionId } })
  );
  acpChatSessionActions.finishSessionLoad(sessionId, session);

  return session;
}

async function loadSession(sessionId: string, options: AcpLoadSessionOptions = {}): Promise<void> {
  const cached = acpChatSessionStore.getSnapshot(sessionId);
  if (cached?.session && !cached.sessionLoadError) {
    window.dispatchEvent(
      new CustomEvent(AppEvents.SESSION_EXTENSIONS_LOADED, { detail: { sessionId } })
    );
    options.onSessionLoaded?.();
    return;
  }

  await loadSessionFromServer(sessionId, options);
}

async function restoreSession(sessionId: string): Promise<void> {
  await loadSessionFromServer(sessionId);
}

async function loadSessionFromServer(
  sessionId: string,
  options: AcpLoadSessionOptions = {},
  replayAll = false
): Promise<void> {
  if (!isAcpSessionLoadInFlight(sessionId)) {
    acpChatSessionActions.startSessionLoad(sessionId);
  }

  const startedAt = Date.now();
  try {
    const { sessionInfo, meta } = await acpLoadSession(sessionId, replayAll);

    showExtensionLoadResults(meta.extensionResults);
    window.dispatchEvent(
      new CustomEvent(AppEvents.SESSION_EXTENSIONS_LOADED, { detail: { sessionId } })
    );
    acpChatSessionActions.finishSessionLoad(
      sessionId,
      sessionInfoToSession(sessionInfo, meta),
      meta.replaySkipped ?? 0
    );
    options.onSessionLoaded?.();
  } catch (error) {
    console.error('Failed to load ACP session:', error);
    const progress = describeLoadProgress(sessionId, startedAt);
    acpChatSessionActions.failSessionLoad(
      sessionId,
      progress ? `${formatAcpError(error)} ${progress}` : formatAcpError(error),
      buildSessionLoadDiagnostics(sessionId, startedAt, error)
    );
  }
}

function loadFullSessionHistory(sessionId: string): Promise<void> {
  return loadSessionFromServer(sessionId, {}, true);
}

function describeLoadProgress(sessionId: string, startedAt: number): string | undefined {
  const notifications = acpChatSessionActions.getReplayNotificationCount(sessionId);
  if (notifications === 0) {
    return undefined;
  }
  const messageCount = acpChatSessionStore.getSnapshot(sessionId)?.session?.message_count;
  const seconds = ((Date.now() - startedAt) / 1000).toFixed(1);
  return messageCount
    ? `(received ${notifications} updates for ~${messageCount} stored messages after ${seconds}s)`
    : `(received ${notifications} updates after ${seconds}s)`;
}

function buildSessionLoadDiagnostics(sessionId: string, startedAt: number, error: unknown): string {
  const snapshot = acpChatSessionStore.getSnapshot(sessionId);
  return [
    `sessionId: ${sessionId}`,
    `storedMessages: ${snapshot?.session?.message_count ?? 'unknown'}`,
    `replayUpdatesReceived: ${acpChatSessionActions.getReplayNotificationCount(sessionId)}`,
    `elapsedMs: ${Date.now() - startedAt}`,
    `desktopVersion: ${window.electron.getVersion()}`,
    `error: ${error instanceof Error ? (error.stack ?? error.message) : String(error)}`,
  ].join('\n');
}

async function submitMessage(
  sessionId: string,
  userMessage: Message,
  options: AcpSubmitMessageOptions
): Promise<void> {
  assertNoPendingPromptCancellation(sessionId);

  const snapshot = acpChatSessionStore.getSnapshot(sessionId);
  if (snapshot?.activePromptAttemptId) {
    return;
  }

  const promptAttemptId = uuidv7();
  acpChatSessionActions.startPromptAttempt(sessionId, promptAttemptId);

  try {
    await acpPromptSession(sessionId, userMessage);
    if (acpChatSessionActions.clearPromptCancellation(sessionId, promptAttemptId)) {
      return;
    }
    if (acpChatSessionActions.finishPromptAttemptIfCurrent(sessionId, promptAttemptId)) {
      void options.onFinish();
    }
  } catch (error) {
    if (acpChatSessionActions.clearPromptCancellation(sessionId, promptAttemptId)) {
      return;
    }

    const creditsExhaustedError = parseAcpCreditsExhaustedError(error);
    if (creditsExhaustedError) {
      if (!acpChatSessionActions.isCurrentPromptAttempt(sessionId, promptAttemptId)) {
        return;
      }

      const messages = [
        ...(options.getCurrentSnapshot()?.messages ?? []),
        createAcpCreditsExhaustedMessage(creditsExhaustedError),
      ];
      acpChatSessionActions.setMessages(sessionId, messages);
      if (acpChatSessionActions.finishPromptAttemptIfCurrent(sessionId, promptAttemptId)) {
        void options.onFinish();
      }
      return;
    }

    const submitError = formatAcpError(error);
    if (acpChatSessionActions.finishPromptAttemptIfCurrent(sessionId, promptAttemptId)) {
      void options.onFinish(submitError);
    }
  }
}

function stop(sessionId: string): void {
  const storedPromptAttemptId = acpChatSessionStore.getSnapshot(sessionId)?.activePromptAttemptId;
  const hasStoredAcpPrompt = storedPromptAttemptId !== null && storedPromptAttemptId !== undefined;

  if (hasStoredAcpPrompt) {
    acpChatSessionActions.startPromptCancellation(sessionId, storedPromptAttemptId);
    cancelAcpPermissionRequestsForSession(sessionId);
    cancelAcpElicitationRequestsForSession(sessionId);
    acpCancelPrompt(sessionId).catch((error) => {
      console.warn('Failed to cancel ACP prompt:', error);
    });
    return;
  }

  acpChatSessionActions.setChatState(sessionId, ChatState.Idle);
}

async function updateMessage(
  sessionId: string,
  messageId: string,
  newContent: string,
  editType: 'fork' | 'edit',
  retainedImages: ImageData[],
  options: AcpSubmitMessageOptions
): Promise<void> {
  assertNoPendingPromptCancellation(sessionId);

  const currentSnapshot = options.getCurrentSnapshot();
  const storedSnapshot = acpChatSessionStore.getSnapshot(sessionId);
  const activePromptAttemptId = storedSnapshot?.activePromptAttemptId;
  const currentMessages = currentSnapshot?.messages ?? [];
  const message = currentMessages.find((m) => m.id === messageId);

  if (!message) {
    throw new Error(`Message with id ${messageId} not found in current messages`);
  }

  if (editType === 'fork') {
    await forkSessionWithEditedMessage(sessionId, message, newContent, retainedImages);
    return;
  }

  const editSnapshot = currentSnapshot ?? storedSnapshot;
  const isPendingToolPermission =
    editSnapshot?.chatState === ChatState.WaitingForUserInput &&
    getPendingToolConfirmationIds(editSnapshot?.messages ?? []).size > 0;
  const isIdle = editSnapshot?.chatState === ChatState.Idle;
  const pendingToolPermissionPromptAttemptId = isPendingToolPermission
    ? activePromptAttemptId
    : undefined;
  const canEditInPlace = isIdle || pendingToolPermissionPromptAttemptId != null;

  if (!canEditInPlace) {
    return;
  }

  if (pendingToolPermissionPromptAttemptId != null) {
    const cancellation = acpChatSessionActions.startPromptCancellation(
      sessionId,
      pendingToolPermissionPromptAttemptId
    );
    if (!cancellation) {
      throw new Error('Cannot update message while prompt is active');
    }

    const promptCancellationSettled = acpChatSessionActions.waitForPromptCancellation(
      sessionId,
      pendingToolPermissionPromptAttemptId
    );

    try {
      await acpCancelPrompt(sessionId);
    } catch {
      acpChatSessionActions.restorePromptCancellation(
        sessionId,
        pendingToolPermissionPromptAttemptId
      );
      throw new Error('Cannot update message because the active prompt could not be cancelled');
    }

    cancelAcpPermissionRequestsForSession(sessionId);
    cancelAcpElicitationRequestsForSession(sessionId);
    await promptCancellationSettled;
  }

  acpChatSessionActions.setChatState(sessionId, ChatState.Thinking);

  try {
    await acpTruncateSessionConversation(sessionId, message.created);

    const truncatedMessages = currentMessages.filter((m) => m.created < message.created);
    const updatedUserMessage = createUserMessage(newContent, retainedImages);

    const messagesForUI = [...truncatedMessages, updatedUserMessage];
    acpChatSessionActions.setMessages(sessionId, messagesForUI);

    await submitMessage(sessionId, updatedUserMessage, options);
  } catch (error) {
    acpChatSessionActions.setChatState(sessionId, ChatState.Idle);
    throw error;
  }
}

export const acpChatSessionController: AcpChatSessionController = {
  createSession,
  loadSession,
  loadFullSessionHistory,
  restoreSession,
  submitMessage,
  stop,
  updateMessage,
};
