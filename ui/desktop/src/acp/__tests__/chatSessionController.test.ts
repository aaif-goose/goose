import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Message } from '../../types/message';
import { ChatState } from '../../types/chatState';
import type { Session } from '../../types/session';
import { acpChatSessionController } from '../chatSessionController';
import {
  acpChatSessionActions,
  acpChatSessionStore,
  type AcpChatSessionSnapshot,
} from '../chatSessionStore';
import { acpCancelPrompt, acpPromptSession } from '../prompt';
import {
  acpLoadSession,
  acpTruncateSessionConversation,
  isAcpSessionLoadInFlight,
  sessionInfoToSession,
} from '../sessions';

vi.mock('../../utils/extensionErrorUtils', () => ({
  showExtensionLoadResults: vi.fn(),
}));

vi.mock('../chatSessionStore', () => ({
  acpChatSessionStore: {
    getSnapshot: vi.fn(),
  },
  acpChatSessionActions: {
    startSessionLoad: vi.fn(),
    finishSessionLoad: vi.fn(),
    failSessionLoad: vi.fn(),
    startPromptAttempt: vi.fn(),
    finishPromptAttemptIfCurrent: vi.fn(),
    isCurrentPromptAttempt: vi.fn(),
    setMessages: vi.fn(),
    addPendingLocalSteerMessage: vi.fn(),
    clearActivePromptAttempt: vi.fn(),
    startPromptCancellation: vi.fn(),
    clearPromptCancellation: vi.fn(),
    restorePromptCancellation: vi.fn(),
    waitForPromptCancellation: vi.fn(),
    setChatState: vi.fn(),
    setSessionMetadata: vi.fn(),
    setSessionLoadError: vi.fn(),
  },
}));

vi.mock('../sessions', () => ({
  acpLoadSession: vi.fn(),
  isAcpSessionLoadInFlight: vi.fn(),
  sessionInfoToSession: vi.fn(),
  acpForkSession: vi.fn(),
  acpTruncateSessionConversation: vi.fn(),
}));

vi.mock('../prompt', () => ({
  acpCancelPrompt: vi.fn(),
  acpPromptSession: vi.fn(),
}));

const SESSION_ID = 'session-1';

function userMessage(): Message & { id: string } {
  return {
    id: 'message-1',
    role: 'user',
    created: 123,
    content: [{ type: 'text', text: 'Hello' }],
    metadata: { userVisible: true, agentVisible: true },
  };
}

function loadedSession(): Session {
  return {
    id: SESSION_ID,
    name: 'Loaded session',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    working_dir: '/tmp',
    message_count: 0,
    extension_data: {},
    source: 'test',
  } as Session;
}

function mockLoadResult() {
  return {
    sessionInfo: {
      sessionId: SESSION_ID,
      cwd: '/tmp',
      title: 'Loaded session',
      updatedAt: '2026-01-01T00:00:00Z',
    },
    response: {},
    meta: {},
  } as Awaited<ReturnType<typeof acpLoadSession>>;
}

function snapshotWithActivePrompt(activePromptAttemptId: string | null): AcpChatSessionSnapshot {
  return {
    session: undefined,
    messages: [],
    tokenState: {
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      accumulatedInputTokens: 0,
      accumulatedOutputTokens: 0,
      accumulatedTotalTokens: 0,
    },
    notifications: [],
    progressMessage: undefined,
    chatState: activePromptAttemptId ? ChatState.Streaming : ChatState.Idle,
    sessionLoadError: undefined,
    activePromptAttemptId,
    activeRunId: activePromptAttemptId ? 'run-1' : null,
    pendingCancelPromptAttemptId: null,
  };
}

function pendingToolPermissionMessage(): Message & { id: string } {
  return {
    id: 'permission-message-1',
    role: 'assistant',
    created: 124,
    content: [
      {
        type: 'toolConfirmationRequest',
        id: 'tool-call-1',
        toolName: 'developer__shell',
        arguments: {},
        prompt: null,
      },
    ],
    metadata: { userVisible: true, agentVisible: true },
  };
}

describe('acpChatSessionController.loadSession', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue(undefined);
    vi.mocked(acpLoadSession).mockResolvedValue(mockLoadResult());
    vi.mocked(sessionInfoToSession).mockReturnValue(loadedSession());
  });

  it('starts a fresh session load before ACP replays notifications', async () => {
    vi.mocked(isAcpSessionLoadInFlight).mockReturnValue(false);

    await acpChatSessionController.loadSession(SESSION_ID);

    expect(acpChatSessionActions.startSessionLoad).toHaveBeenCalledWith(SESSION_ID);
    expect(acpLoadSession).toHaveBeenCalledWith(SESSION_ID);
    expect(acpChatSessionActions.finishSessionLoad).toHaveBeenCalledWith(
      SESSION_ID,
      loadedSession()
    );
  });

  it('does not reset replay state when joining an in-flight session load', async () => {
    vi.mocked(isAcpSessionLoadInFlight).mockReturnValue(true);

    await acpChatSessionController.loadSession(SESSION_ID);

    expect(acpChatSessionActions.startSessionLoad).not.toHaveBeenCalled();
    expect(acpLoadSession).toHaveBeenCalledWith(SESSION_ID);
    expect(acpChatSessionActions.finishSessionLoad).toHaveBeenCalledWith(
      SESSION_ID,
      loadedSession()
    );
  });

  it('retries a failed cached session load', async () => {
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue({
      ...snapshotWithActivePrompt(null),
      session: loadedSession(),
      sessionLoadError: 'Sign in to your provider, then try again.',
    });
    vi.mocked(isAcpSessionLoadInFlight).mockReturnValue(false);

    await acpChatSessionController.loadSession(SESSION_ID);

    expect(acpChatSessionActions.startSessionLoad).toHaveBeenCalledWith(SESSION_ID);
    expect(acpLoadSession).toHaveBeenCalledWith(SESSION_ID);
  });

  it('restores a cached session from the server', async () => {
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue({
      ...snapshotWithActivePrompt(null),
      session: loadedSession(),
    });
    vi.mocked(isAcpSessionLoadInFlight).mockReturnValue(false);

    await acpChatSessionController.restoreSession(SESSION_ID);

    expect(acpChatSessionActions.startSessionLoad).toHaveBeenCalledWith(SESSION_ID);
    expect(acpLoadSession).toHaveBeenCalledWith(SESSION_ID);
    expect(acpChatSessionActions.finishSessionLoad).toHaveBeenCalledWith(
      SESSION_ID,
      loadedSession()
    );
  });
});

describe('acpChatSessionController.stop', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(acpCancelPrompt).mockResolvedValue(undefined);
  });

  it('marks cancellation pending while clearing visible prompt activity', () => {
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue(
      snapshotWithActivePrompt('attempt-1')
    );

    acpChatSessionController.stop(SESSION_ID);

    expect(acpChatSessionActions.startPromptCancellation).toHaveBeenCalledWith(
      SESSION_ID,
      'attempt-1'
    );
    expect(acpCancelPrompt).toHaveBeenCalledWith(SESSION_ID);
  });
});

describe('acpChatSessionController.submitMessage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue(snapshotWithActivePrompt(null));
    vi.mocked(acpPromptSession).mockResolvedValue({ stopReason: 'cancelled' } as never);
    vi.mocked(acpChatSessionActions.clearPromptCancellation).mockReturnValue(undefined);
    vi.mocked(acpChatSessionActions.finishPromptAttemptIfCurrent).mockReturnValue(true);
  });

  it('clears a pending cancellation barrier when the original prompt settles', async () => {
    vi.mocked(acpChatSessionActions.clearPromptCancellation).mockReturnValueOnce(
      snapshotWithActivePrompt(null)
    );
    const onFinish = vi.fn();

    await acpChatSessionController.submitMessage(SESSION_ID, userMessage(), {
      getCurrentSnapshot: () => snapshotWithActivePrompt(null),
      onFinish,
    });

    expect(acpChatSessionActions.clearPromptCancellation).toHaveBeenCalledWith(
      SESSION_ID,
      expect.any(String)
    );
    expect(acpChatSessionActions.finishPromptAttemptIfCurrent).not.toHaveBeenCalled();
    expect(onFinish).not.toHaveBeenCalled();
  });

  it('rejects while a cancellation barrier is pending', async () => {
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue({
      ...snapshotWithActivePrompt(null),
      pendingCancelPromptAttemptId: 'attempt-1',
    });

    await expect(
      acpChatSessionController.submitMessage(SESSION_ID, userMessage(), {
        getCurrentSnapshot: () => snapshotWithActivePrompt(null),
        onFinish: vi.fn(),
      })
    ).rejects.toThrow('Cannot submit while prompt cancellation is pending');

    expect(acpChatSessionActions.startPromptAttempt).not.toHaveBeenCalled();
    expect(acpPromptSession).not.toHaveBeenCalled();
  });

});

describe('acpChatSessionController.updateMessage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(acpTruncateSessionConversation).mockResolvedValue(undefined as never);
    vi.mocked(acpPromptSession).mockResolvedValue({ stopReason: 'end_turn' } as never);
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue(snapshotWithActivePrompt(null));
    vi.mocked(acpChatSessionActions.waitForPromptCancellation).mockResolvedValue(undefined);
  });

  it('rejects edits before truncating while cancellation is pending', async () => {
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue({
      ...snapshotWithActivePrompt(null),
      pendingCancelPromptAttemptId: 'attempt-1',
    });
    const existingMessage = userMessage();
    const currentSnapshot: AcpChatSessionSnapshot = {
      ...snapshotWithActivePrompt(null),
      messages: [existingMessage],
    };

    await expect(
      acpChatSessionController.updateMessage(
        SESSION_ID,
        existingMessage.id,
        'Updated',
        'edit',
        [],
        {
          getCurrentSnapshot: () => currentSnapshot,
          onFinish: vi.fn(),
        }
      )
    ).rejects.toThrow('Cannot submit while prompt cancellation is pending');

    expect(acpChatSessionActions.setChatState).not.toHaveBeenCalledWith(
      SESSION_ID,
      ChatState.Thinking
    );
    expect(acpTruncateSessionConversation).not.toHaveBeenCalled();
    expect(acpChatSessionActions.setMessages).not.toHaveBeenCalled();
    expect(acpPromptSession).not.toHaveBeenCalled();
  });

  it('ignores edits before truncating while a prompt is active', async () => {
    vi.mocked(acpChatSessionStore.getSnapshot).mockReturnValue(
      snapshotWithActivePrompt('attempt-1')
    );
    const existingMessage = userMessage();
    const currentSnapshot: AcpChatSessionSnapshot = {
      ...snapshotWithActivePrompt('attempt-1'),
      messages: [existingMessage],
    };

    await expect(
      acpChatSessionController.updateMessage(
        SESSION_ID,
        existingMessage.id,
        'Updated',
        'edit',
        [],
        {
          getCurrentSnapshot: () => currentSnapshot,
          onFinish: vi.fn(),
        }
      )
    ).resolves.toBeUndefined();

    expect(acpChatSessionActions.setChatState).not.toHaveBeenCalledWith(
      SESSION_ID,
      ChatState.Thinking
    );
    expect(acpTruncateSessionConversation).not.toHaveBeenCalled();
    expect(acpChatSessionActions.setMessages).not.toHaveBeenCalled();
    expect(acpPromptSession).not.toHaveBeenCalled();
  });

  it('waits for pending tool permission cancellation before truncating and rerunning', async () => {
    const existingMessage = userMessage();
    const permissionMessage = pendingToolPermissionMessage();
    const activeSnapshot: AcpChatSessionSnapshot = {
      ...snapshotWithActivePrompt('attempt-1'),
      chatState: ChatState.WaitingForUserInput,
      messages: [existingMessage, permissionMessage],
    };
    let storedSnapshot = activeSnapshot;
    vi.mocked(acpChatSessionStore.getSnapshot).mockImplementation(() => storedSnapshot);
    vi.mocked(acpChatSessionActions.startPromptCancellation).mockReturnValue({
      ...activeSnapshot,
      activePromptAttemptId: null,
      pendingCancelPromptAttemptId: 'attempt-1',
    });
    vi.mocked(acpCancelPrompt).mockResolvedValue(undefined);

    let resolvePromptCancellation: () => void;
    const promptCancellationSettled = new Promise<void>((resolve) => {
      resolvePromptCancellation = resolve;
    });
    vi.mocked(acpChatSessionActions.waitForPromptCancellation).mockReturnValue(
      promptCancellationSettled
    );

    const updatePromise = acpChatSessionController.updateMessage(
      SESSION_ID,
      existingMessage.id,
      'Updated',
      'edit',
      [],
      {
        getCurrentSnapshot: () => activeSnapshot,
        onFinish: vi.fn(),
      }
    );

    await Promise.resolve();
    await Promise.resolve();

    expect(acpCancelPrompt).toHaveBeenCalledWith(SESSION_ID);
    expect(acpChatSessionActions.waitForPromptCancellation).toHaveBeenCalledWith(
      SESSION_ID,
      'attempt-1'
    );
    expect(acpTruncateSessionConversation).not.toHaveBeenCalled();
    expect(acpPromptSession).not.toHaveBeenCalled();

    storedSnapshot = {
      ...snapshotWithActivePrompt(null),
      messages: [existingMessage, permissionMessage],
    };
    resolvePromptCancellation!();
    await updatePromise;

    expect(acpTruncateSessionConversation).toHaveBeenCalledWith(
      SESSION_ID,
      existingMessage.created
    );
    expect(acpPromptSession).toHaveBeenCalled();
    expect(acpChatSessionActions.clearPromptCancellation).not.toHaveBeenCalledWith(
      SESSION_ID,
      'attempt-1'
    );
  });
});
