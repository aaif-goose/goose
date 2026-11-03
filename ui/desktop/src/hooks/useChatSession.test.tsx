import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

type TestSnapshot = {
  session: { id: string };
  messages: unknown[];
  chatState: string;
  pendingCancelPromptAttemptId: string | null;
  activeRunId: string | null;
  activePromptAttemptId: string | null;
};

const mocks = vi.hoisted(() => ({
  snapshot: undefined as TestSnapshot | undefined,
  submitMessage: vi.fn(() => {
    if (mocks.snapshot) {
      mocks.snapshot.activePromptAttemptId = 'attempt-1';
      mocks.snapshot.chatState = 'streaming';
    }
    return Promise.resolve();
  }),
  modelChangePending: false,
  listeners: new Set<() => void>(),
}));

vi.mock('../acp/chatSessionController', () => ({
  acpChatSessionController: {
    submitMessage: mocks.submitMessage,
    loadSession: vi.fn(),
    restoreSession: vi.fn(),
  },
}));

vi.mock('../acp/chatSessionStore', () => ({
  useAcpChatSessionSnapshot: () => mocks.snapshot,
  acpChatSessionStore: { getSnapshot: () => mocks.snapshot },
  acpChatSessionActions: { setMessages: vi.fn() },
}));

vi.mock('../acp/acpConnection', () => ({
  isAcpRecovering: () => false,
}));

vi.mock('../components/ModelAndProviderContext', () => ({
  isModelChangePendingNow: () => mocks.modelChangePending,
  subscribeModelChangePending: (listener: () => void) => {
    mocks.listeners.add(listener);
    return () => {
      mocks.listeners.delete(listener);
    };
  },
}));

import { clearDeferredSubmits } from './deferredSessionSubmits';
import { useChatSession } from './useChatSession';
import { IntlTestWrapper } from '../i18n/test-utils';
import { ChatState } from '../types/chatState';
import { createUserMessage, type Message } from '../types/message';

function assistantMessage(text: string): Message {
  return {
    id: 'assistant-1',
    role: 'assistant',
    created: 0,
    content: [{ type: 'text', text }],
    metadata: { userVisible: true, agentVisible: true },
  };
}

function idleSnapshot(messages: Message[]) {
  return {
    session: { id: 'sess-1' },
    messages,
    chatState: ChatState.Idle,
    pendingCancelPromptAttemptId: null,
    activeRunId: null,
    activePromptAttemptId: null,
  };
}

function releaseTurn() {
  if (!mocks.snapshot) {
    throw new Error('snapshot missing');
  }
  mocks.snapshot.activePromptAttemptId = null;
  mocks.snapshot.chatState = ChatState.Idle;
}

function renderChatSession(messages: Message[], resetSnapshot = true) {
  if (resetSnapshot) {
    mocks.snapshot = idleSnapshot(messages);
  }
  return renderHook(() => useChatSession({ sessionId: 'sess-1', onStreamFinish: vi.fn() }), {
    wrapper: IntlTestWrapper,
  });
}

describe('useChatSession handleSubmit', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearDeferredSubmits();
    mocks.modelChangePending = false;
    mocks.listeners.clear();
    mocks.submitMessage.mockImplementation(() => {
      if (mocks.snapshot) {
        mocks.snapshot.activePromptAttemptId = 'attempt-1';
        mocks.snapshot.chatState = ChatState.Streaming;
      }
      return Promise.resolve();
    });
  });

  it('does not send the last assistant message as a new prompt on an empty submit', async () => {
    const { result } = renderChatSession([
      createUserMessage('Question'),
      assistantMessage('Answer'),
    ]);

    await act(() => result.current.handleSubmit({ msg: '', images: [] }));

    expect(mocks.submitMessage).not.toHaveBeenCalled();
  });

  it('does not submit while a model change is still applying', async () => {
    mocks.modelChangePending = true;
    const question = createUserMessage('Question');
    const { result } = renderChatSession([question]);

    await act(() => result.current.handleSubmit({ msg: 'next', images: [] }));

    expect(mocks.submitMessage).not.toHaveBeenCalled();
  });

  it('sends a submit that arrived during a model change once that change settles', async () => {
    mocks.modelChangePending = true;
    const question = createUserMessage('Question');
    const { result } = renderChatSession([question]);

    await act(() => result.current.handleSubmit({ msg: 'from recipe', images: [] }));
    expect(mocks.submitMessage).not.toHaveBeenCalled();

    mocks.modelChangePending = false;
    await act(() => {
      for (const listener of mocks.listeners) listener();
    });

    expect(mocks.submitMessage).toHaveBeenCalledTimes(1);
    expect(mocks.submitMessage).toHaveBeenCalledWith(
      'sess-1',
      expect.objectContaining({
        content: [expect.objectContaining({ type: 'text', text: 'from recipe' })],
      }),
      expect.anything()
    );
  });

  it('sends deferred submits one turn at a time', async () => {
    mocks.modelChangePending = true;
    const question = createUserMessage('Question');
    const { result, rerender } = renderChatSession([question]);

    await act(() => result.current.handleSubmit({ msg: 'first', images: [] }));
    await act(() => result.current.handleSubmit({ msg: 'second', images: [] }));

    mocks.modelChangePending = false;
    await act(() => {
      for (const listener of mocks.listeners) listener();
    });

    expect(mocks.submitMessage).toHaveBeenCalledTimes(1);
    expect(mocks.submitMessage).toHaveBeenCalledWith(
      'sess-1',
      expect.objectContaining({
        content: [expect.objectContaining({ type: 'text', text: 'first' })],
      }),
      expect.anything()
    );

    rerender();
    expect(mocks.submitMessage).toHaveBeenCalledTimes(1);

    releaseTurn();
    rerender();

    expect(mocks.submitMessage).toHaveBeenCalledTimes(2);
    expect(mocks.submitMessage).toHaveBeenLastCalledWith(
      'sess-1',
      expect.objectContaining({
        content: [expect.objectContaining({ type: 'text', text: 'second' })],
      }),
      expect.anything()
    );
  });

  it('does not drain a deferred submit while a restored run is still active', async () => {
    const question = createUserMessage('Question');
    mocks.snapshot = { ...idleSnapshot([question]), activeRunId: 'run-1' };
    mocks.modelChangePending = true;
    const { result, rerender } = renderChatSession([question], false);

    await act(() => result.current.handleSubmit({ msg: 'after restore', images: [] }));
    mocks.modelChangePending = false;
    await act(() => {
      for (const listener of mocks.listeners) listener();
    });

    expect(mocks.submitMessage).not.toHaveBeenCalled();

    if (!mocks.snapshot) {
      throw new Error('snapshot missing');
    }
    mocks.snapshot.activeRunId = null;
    rerender();

    expect(mocks.submitMessage).toHaveBeenCalledTimes(1);
    expect(mocks.submitMessage).toHaveBeenCalledWith(
      'sess-1',
      expect.objectContaining({
        content: [expect.objectContaining({ type: 'text', text: 'after restore' })],
      }),
      expect.anything()
    );
  });

  it('keeps deferred submits when the chat hook remounts', async () => {
    mocks.modelChangePending = true;
    const question = createUserMessage('Question');
    const first = renderChatSession([question]);

    await act(() => first.result.current.handleSubmit({ msg: 'from recipe', images: [] }));
    await act(() => first.result.current.handleSubmit({ msg: 'after remount', images: [] }));
    first.unmount();

    mocks.modelChangePending = false;
    const second = renderChatSession([question]);

    expect(mocks.submitMessage).toHaveBeenCalledTimes(1);
    expect(mocks.submitMessage).toHaveBeenCalledWith(
      'sess-1',
      expect.objectContaining({
        content: [expect.objectContaining({ type: 'text', text: 'from recipe' })],
      }),
      expect.anything()
    );

    second.unmount();
    const remountedDuringTurn = renderChatSession([question], false);
    expect(mocks.submitMessage).toHaveBeenCalledTimes(1);

    releaseTurn();
    remountedDuringTurn.rerender();

    expect(mocks.submitMessage).toHaveBeenCalledTimes(2);
    expect(mocks.submitMessage).toHaveBeenLastCalledWith(
      'sess-1',
      expect.objectContaining({
        content: [expect.objectContaining({ type: 'text', text: 'after remount' })],
      }),
      expect.anything()
    );
  });

  it('sends the last user message again on an empty submit', async () => {
    const question = createUserMessage('Question');
    const { result } = renderChatSession([question]);

    await act(() => result.current.handleSubmit({ msg: '', images: [] }));

    expect(mocks.submitMessage).toHaveBeenCalledTimes(1);
    expect(mocks.submitMessage).toHaveBeenCalledWith('sess-1', question, expect.anything());
  });
});
