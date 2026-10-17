import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import type { ImageData, Message, MessageContent } from '../types/message';
import { IntlTestWrapper } from '../i18n/test-utils';
import MessageList from './MessageList';

const renderCounts = vi.hoisted(() => new Map<string, number>());
const messageUpdateCallbacks = vi.hoisted(
  () =>
    new Map<
      string,
      | ((
          messageId: string,
          newContent: string,
          editType: 'fork' | 'edit',
          retainedImages: ImageData[]
        ) => void)
      | undefined
    >()
);

vi.mock('./GooseMessage', () => ({
  default: ({ message }: { message: Message }) => {
    const id = message.id ?? 'missing-id';
    renderCounts.set(id, (renderCounts.get(id) ?? 0) + 1);
    return <div>{id}</div>;
  },
}));

vi.mock('./UserMessage', () => ({
  default: ({
    message,
    onMessageUpdate,
  }: {
    message: Message;
    onMessageUpdate?: (
      messageId: string,
      newContent: string,
      editType: 'fork' | 'edit',
      retainedImages: ImageData[]
    ) => void;
  }) => {
    const id = message.id ?? 'missing-id';
    renderCounts.set(id, (renderCounts.get(id) ?? 0) + 1);
    messageUpdateCallbacks.set(id, onMessageUpdate);
    return <div>{id}</div>;
  },
}));

const visibleMetadata: Message['metadata'] = { agentVisible: true, userVisible: true };
const append = vi.fn();
const isUserMessage = (message: Message) => message.role === 'user';

function message(id: string, role: Message['role'], content: MessageContent[]): Message {
  return { id, role, created: 1, content, metadata: visibleMetadata };
}

function cloneMessages(messages: Message[]): Message[] {
  return messages.map((item) => ({
    ...item,
    content: item.content.map((content) => ({ ...content })),
    metadata: { ...item.metadata },
  }));
}

function toolRequest(id: string): MessageContent {
  return {
    type: 'toolRequest',
    id,
    toolCall: {
      status: 'success',
      value: { name: 'test_tool', arguments: {} },
    },
  };
}

function toolResponse(id: string): MessageContent {
  return {
    type: 'toolResponse',
    id,
    toolResult: {
      status: 'success',
      value: { content: [{ type: 'text', text: 'complete' }], isError: false },
    },
  };
}

function renderList(messages: Message[]) {
  return render(
    <MessageList
      messages={messages}
      sessionId="test-session"
      append={append}
      isUserMessage={isUserMessage}
    />,
    { wrapper: IntlTestWrapper }
  );
}

beforeEach(() => {
  vi.mocked(window.electron.on).mockClear();
  vi.mocked(window.electron.off).mockClear();
});

describe('MessageList render isolation', () => {
  beforeEach(() => {
    renderCounts.clear();
    messageUpdateCallbacks.clear();
    append.mockClear();
  });

  it('does not rerender historical rows from cloned equivalent messages', () => {
    const messages = [
      message('assistant-1', 'assistant', [{ type: 'text', text: 'First' }]),
      message('user-1', 'user', [{ type: 'text', text: 'Continue' }]),
      message('assistant-2', 'assistant', [{ type: 'text', text: 'Streaming' }]),
    ];
    const { rerender } = renderList(messages);

    rerender(
      <MessageList
        messages={cloneMessages(messages)}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
      />
    );

    expect(renderCounts).toEqual(
      new Map([
        ['assistant-1', 1],
        ['user-1', 1],
        ['assistant-2', 1],
      ])
    );

    const updatedMessages = cloneMessages(messages);
    updatedMessages[2].content = [{ type: 'text', text: 'Streaming update' }];
    rerender(
      <MessageList
        messages={updatedMessages}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
      />
    );

    expect(renderCounts).toEqual(
      new Map([
        ['assistant-1', 1],
        ['user-1', 1],
        ['assistant-2', 2],
      ])
    );
  });

  it('rerenders the matching request row when a tool response arrives', () => {
    const messages = [
      message('tool-request', 'assistant', [toolRequest('tool-1')]),
      message('unrelated', 'assistant', [{ type: 'text', text: 'Unrelated' }]),
      message('tool-response', 'user', []),
    ];
    const { rerender } = renderList(messages);
    const updatedMessages = cloneMessages(messages);
    updatedMessages[2].content = [toolResponse('tool-1')];

    rerender(
      <MessageList
        messages={updatedMessages}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
      />
    );

    expect(renderCounts.get('tool-request')).toBe(2);
    expect(renderCounts.get('unrelated')).toBe(1);
  });

  it('preserves the message update callback', () => {
    const onMessageUpdate = vi.fn();
    render(
      <MessageList
        messages={[message('user-1', 'user', [{ type: 'text', text: 'Original' }])]}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
        onMessageUpdate={onMessageUpdate}
      />,
      { wrapper: IntlTestWrapper }
    );
    const retainedImages: ImageData[] = [{ data: 'image', mimeType: 'image/png' }];

    messageUpdateCallbacks.get('user-1')?.('user-1', 'Updated', 'fork', retainedImages);

    expect(onMessageUpdate).toHaveBeenCalledWith('user-1', 'Updated', 'fork', retainedImages);
  });
});

describe('MessageList transcript window', () => {
  function messages(count: number): Message[] {
    return Array.from({ length: count }, (_, index) =>
      message(`assistant-${index}`, 'assistant', [{ type: 'text', text: `Message ${index}` }])
    );
  }

  it('renders the latest messages immediately and hides the middle', () => {
    renderList(messages(500));

    expect(screen.queryByText('assistant-0')).not.toBeNull();
    expect(screen.queryByText('assistant-19')).not.toBeNull();
    expect(screen.queryByText('assistant-20')).toBeNull();
    expect(screen.queryByText('assistant-299')).toBeNull();
    expect(screen.queryByText('assistant-300')).not.toBeNull();
    expect(screen.queryByText('assistant-499')).not.toBeNull();
    expect(screen.queryByText('280 messages hidden')).not.toBeNull();
  });

  it('counts and windows visible messages', () => {
    const transcript = messages(300).map((item, index) =>
      index >= 100 && index < 160
        ? { ...item, metadata: { ...item.metadata, userVisible: false } }
        : item
    );

    renderList(transcript);

    expect(screen.queryByText('assistant-20')).toBeNull();
    expect(screen.queryByText('assistant-39')).toBeNull();
    expect(screen.queryByText('assistant-40')).not.toBeNull();
    expect(screen.queryByText('assistant-100')).toBeNull();
    expect(screen.queryByText('20 messages hidden')).not.toBeNull();
  });

  it('renders the full conversation when requested', () => {
    renderList(messages(221));

    expect(screen.queryByText('assistant-20')).toBeNull();

    act(() => screen.getByRole('button', { name: 'Show all' }).click());

    expect(screen.queryByText('assistant-20')).not.toBeNull();
    expect(screen.queryByText(/message hidden/)).toBeNull();
  });

  it('renders the full conversation when search opens from the keyboard', () => {
    renderList(messages(221));

    act(() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: true })));

    expect(screen.queryByText('assistant-20')).not.toBeNull();
  });

  it('renders the full conversation when search opens from the menu', () => {
    renderList(messages(221));
    const findHandler = vi
      .mocked(window.electron.on)
      .mock.calls.find(([event]) => event === 'find-command')?.[1];

    act(() => findHandler?.({} as Electron.IpcRendererEvent));

    expect(screen.queryByText('assistant-20')).not.toBeNull();
  });

  it('uses the hidden messages when deriving row context', () => {
    const transcript = messages(221).map((item, index) => ({
      ...item,
      metadata: {
        ...item.metadata,
        inference: { resolvedModel: index < 20 ? 'old-model' : 'new-model' },
      },
    }));

    renderList(transcript);

    expect(screen.queryByText('Model changed: old-model → new-model')).toBeNull();
  });
});
