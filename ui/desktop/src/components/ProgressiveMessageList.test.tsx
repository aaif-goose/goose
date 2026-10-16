import { StrictMode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import type { ImageData, Message, MessageContent } from '../types/message';
import { IntlTestWrapper } from '../i18n/test-utils';
import ProgressiveMessageList from './ProgressiveMessageList';

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
    <ProgressiveMessageList
      messages={messages}
      sessionId="test-session"
      append={append}
      isUserMessage={isUserMessage}
    />,
    { wrapper: IntlTestWrapper }
  );
}

describe('ProgressiveMessageList render isolation', () => {
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
      <ProgressiveMessageList
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
      <ProgressiveMessageList
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
      <ProgressiveMessageList
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
      <ProgressiveMessageList
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

describe('ProgressiveMessageList batching', () => {
  const messages = Array.from({ length: 10 }, (_, index) =>
    message(`assistant-${index}`, 'assistant', [{ type: 'text', text: `Message ${index}` }])
  );

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function batchedList(
    listMessages: Message[],
    { sessionId = 'test-session', onRenderingComplete = vi.fn() } = {}
  ) {
    return (
      <StrictMode>
        <IntlTestWrapper>
          <ProgressiveMessageList
            messages={listMessages}
            sessionId={sessionId}
            append={append}
            isUserMessage={isUserMessage}
            batchSize={2}
            batchDelay={20}
            showLoadingThreshold={0}
            onRenderingComplete={onRenderingComplete}
          />
        </IntlTestWrapper>
      </StrictMode>
    );
  }

  function renderedMessageIds() {
    return screen.queryAllByTestId('message-container').map((row) => row.textContent);
  }

  function messageIds(prefix: string, from: number, to: number) {
    return Array.from({ length: to - from + 1 }, (_, offset) => `${prefix}-${from + offset}`);
  }

  it('renders the newest batch first, then one older batch per delay in StrictMode', () => {
    render(batchedList(messages));

    expect(renderedMessageIds()).toEqual(messageIds('assistant', 8, 9));

    act(() => vi.advanceTimersByTime(20));
    expect(renderedMessageIds()).toEqual(messageIds('assistant', 6, 9));

    act(() => vi.advanceTimersByTime(20));
    expect(renderedMessageIds()).toEqual(messageIds('assistant', 4, 9));
  });

  it('shows the loading banner above the rendered messages', () => {
    render(batchedList(messages));

    const banner = screen.getByText('Loading messages... (2/10)');
    const newestMessage = screen.getByText('assistant-8');

    expect(banner.compareDocumentPosition(newestMessage)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
  });

  it('reports completion once after the final batch', () => {
    const onRenderingComplete = vi.fn();
    render(batchedList(messages, { onRenderingComplete }));

    for (let batch = 0; batch < 4; batch++) {
      act(() => vi.advanceTimersByTime(20));
    }
    expect(renderedMessageIds()).toEqual(messageIds('assistant', 0, 9));
    expect(onRenderingComplete).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(50));
    expect(onRenderingComplete).toHaveBeenCalledTimes(1);
  });

  it('keeps the rendered messages when a new message arrives while older batches load', () => {
    const { rerender } = render(batchedList(messages));
    act(() => vi.advanceTimersByTime(20));

    rerender(
      batchedList([
        ...messages,
        message('assistant-10', 'assistant', [{ type: 'text', text: 'New message' }]),
      ])
    );
    expect(renderedMessageIds()).toEqual(messageIds('assistant', 6, 10));
    expect(screen.queryByText('Loading messages... (5/11)')).not.toBeNull();

    act(() => vi.advanceTimersByTime(20));
    expect(renderedMessageIds()).toEqual(messageIds('assistant', 4, 10));
  });

  it('starts again from the newest messages when the session changes', () => {
    const { rerender } = render(batchedList(messages));
    for (let batch = 0; batch < 4; batch++) {
      act(() => vi.advanceTimersByTime(20));
    }
    expect(renderedMessageIds()).toEqual(messageIds('assistant', 0, 9));

    const otherSessionMessages = Array.from({ length: 10 }, (_, index) =>
      message(`other-${index}`, 'assistant', [{ type: 'text', text: `Other ${index}` }])
    );
    rerender(batchedList(otherSessionMessages, { sessionId: 'other-session' }));

    expect(renderedMessageIds()).toEqual(messageIds('other', 8, 9));
  });

  it('starts again from the newest messages when the conversation reloads', () => {
    const { rerender } = render(batchedList(messages));
    for (let batch = 0; batch < 4; batch++) {
      act(() => vi.advanceTimersByTime(20));
    }

    rerender(batchedList([]));
    rerender(batchedList(messages));

    expect(renderedMessageIds()).toEqual(messageIds('assistant', 8, 9));
  });

  it('shows a model change at the message where it happened', () => {
    const modelChangeIndex = 7;
    const messagesWithModels = messages.map((item, index) => ({
      ...item,
      metadata: {
        ...item.metadata,
        inference: {
          provider: 'test-provider',
          requestedModel: 'model-a',
          resolvedModel: index < modelChangeIndex ? 'model-a' : 'model-b',
        },
      },
    }));
    render(batchedList(messagesWithModels));

    expect(screen.queryByText('Model changed: model-a → model-b')).toBeNull();

    act(() => vi.advanceTimersByTime(20));
    const modelChange = screen.getAllByText('Model changed: model-a → model-b');
    expect(modelChange).toHaveLength(1);
    expect(modelChange[0].compareDocumentPosition(screen.getByText('assistant-7'))).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING
    );
  });
});
