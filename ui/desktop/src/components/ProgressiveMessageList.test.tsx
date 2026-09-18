import { StrictMode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import type { ImageData, Message, MessageContent } from '../types/message';
import { IntlTestWrapper } from '../i18n/test-utils';
import ProgressiveMessageList, {
  HEAD_MESSAGE_COUNT,
  MESSAGE_DISPLAY_LIMIT,
  TAIL_MESSAGE_COUNT,
} from './ProgressiveMessageList';

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

  function renderBatchedList(onRenderingComplete = vi.fn()) {
    render(
      <StrictMode>
        <IntlTestWrapper>
          <ProgressiveMessageList
            messages={messages}
            sessionId="test-session"
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
    return onRenderingComplete;
  }

  it('renders exactly one batch per delay in StrictMode', () => {
    renderBatchedList();

    expect(screen.queryByText('assistant-1')).not.toBeNull();
    expect(screen.queryByText('assistant-2')).toBeNull();

    act(() => vi.advanceTimersByTime(20));
    expect(screen.queryByText('assistant-3')).not.toBeNull();
    expect(screen.queryByText('assistant-4')).toBeNull();

    act(() => vi.advanceTimersByTime(20));
    expect(screen.queryByText('assistant-5')).not.toBeNull();
    expect(screen.queryByText('assistant-6')).toBeNull();
  });

  it('reports completion once after the final batch', () => {
    const onRenderingComplete = renderBatchedList();

    for (let batch = 0; batch < 4; batch++) {
      act(() => vi.advanceTimersByTime(20));
    }
    expect(screen.queryByText('assistant-9')).not.toBeNull();
    expect(onRenderingComplete).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(50));
    expect(onRenderingComplete).toHaveBeenCalledTimes(1);
  });
});

describe('ProgressiveMessageList truncation', () => {
  // One user message per turn, so every index is its own turn boundary and the
  // window lands exactly on HEAD_MESSAGE_COUNT / TAIL_MESSAGE_COUNT.
  function turns(count: number): Message[] {
    return Array.from({ length: count }, (_, index) =>
      message(`user-${index}`, 'user', [{ type: 'text', text: `Message ${index}` }])
    );
  }

  function renderAll(messages: Message[]) {
    return render(
      <ProgressiveMessageList
        messages={messages}
        sessionId="test-session"
        append={append}
        isUserMessage={isUserMessage}
        showLoadingThreshold={messages.length}
      />,
      { wrapper: IntlTestWrapper }
    );
  }

  it('renders every message at the truncation limit', () => {
    renderAll(turns(MESSAGE_DISPLAY_LIMIT));

    expect(screen.queryByText(`user-${HEAD_MESSAGE_COUNT}`)).not.toBeNull();
    expect(screen.queryByText(/messages omitted/)).toBeNull();
  });

  it('keeps the first and last turns and omits the middle past the limit', () => {
    const total = MESSAGE_DISPLAY_LIMIT + 1;
    renderAll(turns(total));

    expect(screen.queryByText('user-0')).not.toBeNull();
    expect(screen.queryByText(`user-${HEAD_MESSAGE_COUNT - 1}`)).not.toBeNull();
    expect(screen.queryByText(`user-${HEAD_MESSAGE_COUNT}`)).toBeNull();
    expect(screen.queryByText(`user-${total - TAIL_MESSAGE_COUNT - 1}`)).toBeNull();
    expect(screen.queryByText(`user-${total - TAIL_MESSAGE_COUNT}`)).not.toBeNull();
    expect(screen.queryByText(`user-${total - 1}`)).not.toBeNull();

    const omitted = total - TAIL_MESSAGE_COUNT - HEAD_MESSAGE_COUNT;
    expect(screen.queryByText(`${omitted} earlier messages omitted`)).not.toBeNull();
  });

  it('never splits a tool request from its response', () => {
    const messages = turns(MESSAGE_DISPLAY_LIMIT + 1);
    const tailBoundary = messages.length - TAIL_MESSAGE_COUNT;
    // An unsnapped window would start on the response and cut off its request.
    messages[tailBoundary - 1] = message('assistant-call', 'assistant', [toolRequest('tool-1')]);
    messages[tailBoundary] = message('user-response', 'user', [toolResponse('tool-1')]);

    renderAll(messages);

    expect(screen.queryByText('assistant-call')).not.toBeNull();
    expect(screen.queryByText(`user-${tailBoundary - 2}`)).not.toBeNull();
    expect(screen.queryByText(`user-${tailBoundary - 3}`)).toBeNull();
  });

  it('shows everything once the user asks for all messages', () => {
    renderAll(turns(MESSAGE_DISPLAY_LIMIT + 1));

    act(() => {
      screen.getByRole('button', { name: 'Show all' }).click();
    });

    expect(screen.queryByText(`user-${HEAD_MESSAGE_COUNT}`)).not.toBeNull();
    expect(screen.queryByText(/messages omitted/)).toBeNull();
  });
});
