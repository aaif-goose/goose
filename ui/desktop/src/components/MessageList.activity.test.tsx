import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import type { Message, MessageContent } from '../types/message';
import { formatMessageTimestamp } from '../utils/timeUtils';
import MessageList from './MessageList';
import { deriveMessageRowContexts } from './messageRowContext';
import { deriveTurnActivity } from './turnActivity';
import { useTurnActivity } from './useTurnActivity';

const resolvePermission = vi.hoisted(() => vi.fn(() => true));
vi.mock('../acp/permissionRequests', () => ({ resolveAcpPermissionRequest: resolvePermission }));
vi.mock('./McpApps/McpAppRenderer', () => ({
  default: () => <iframe title="Draft app" />,
}));

const isUserMessage = (message: Message) => message.role === 'user';

function message(id: string, role: Message['role'], ...content: MessageContent[]): Message {
  return {
    id,
    role,
    content,
    created: 1790000000,
    metadata: { agentVisible: true, userVisible: true },
  };
}

function text(text: string): MessageContent {
  return { type: 'text', text };
}

function request(id: string, name: string): MessageContent {
  return {
    type: 'toolRequest',
    id,
    toolCall: { status: 'success', value: { name, arguments: {} } },
  };
}

function response(id: string, type: 'success' | 'error' | 'app' = 'success'): MessageContent {
  return {
    type: 'toolResponse',
    id,
    toolResult:
      type === 'error'
        ? { status: 'error', error: 'Tool failed' }
        : {
            status: 'success',
            value: {
              content: [{ type: 'text', text: 'Tool result' }],
              isError: false,
              ...(type === 'app' && {
                _meta: {
                  ui: { resourceUri: 'ui://draft' },
                  extensionName: 'demo',
                  toolName: 'draft',
                },
              }),
            },
          },
  };
}

function Transcript({ messages, active = false }: { messages: Message[]; active?: boolean }) {
  const activity = useTurnActivity(messages, isUserMessage, active);
  return (
    <MessageList
      activity={activity}
      messages={messages}
      sessionId="session"
      isUserMessage={isUserMessage}
      isStreamingMessage={active}
    />
  );
}

const prompt = message('user', 'user', text('Run the checks'));

describe('turn activity disclosure', () => {
  it('keeps the final answer, media, and notification inline and expands steps in order', async () => {
    const final = message('final', 'assistant', text('The checks passed.'), {
      type: 'systemNotification',
      notificationType: 'inlineMessage',
      msg: 'Reply annotation',
    });
    final.metadata.usage = { totalTokens: 42 };
    render(
      <Transcript
        messages={[
          prompt,
          message(
            'first',
            'assistant',
            { type: 'thinking', thinking: 'Plan the checks', signature: '' },
            text('Reading the file.'),
            request('read', 'read_file')
          ),
          message('read-result', 'user', response('read')),
          message(
            'second',
            'assistant',
            text('Checking the result.'),
            request('verify', 'verify_result')
          ),
          message('verify-result', 'user', response('verify')),
          final,
          message('notice', 'assistant', {
            type: 'systemNotification',
            notificationType: 'inlineMessage',
            msg: 'A notification',
          }),
          message('image', 'assistant', { type: 'image', data: 'AQID', mimeType: 'image/png' }),
        ]}
      />,
      { wrapper: IntlTestWrapper }
    );

    const trigger = screen.getByRole('button', { name: /2 tool calls.*Completed/ });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(screen.queryByText('Reading the file.')).toBeNull();
    expect(screen.getByText('The checks passed.')).toBeVisible();
    expect(screen.getByText('Reply annotation')).toBeVisible();
    expect(screen.getByText('A notification')).toBeVisible();
    expect(screen.getByRole('img', { name: /uploaded|preview|image/i })).toBeVisible();
    const answerRow = screen
      .getByText('The checks passed.')
      .closest('[data-testid="message-container"]');
    expect(answerRow).not.toBeNull();
    expect(
      within(answerRow as HTMLElement).getByText(formatMessageTimestamp(final.created))
    ).toBeInTheDocument();

    fireEvent.click(trigger);
    const thinking = await screen.findByRole('button', { name: 'Thinking' });
    const firstText = screen.getByText('Reading the file.');
    const firstTool = screen.getByText('Read File');
    const secondText = screen.getByText('Checking the result.');
    const secondTool = screen.getByText('Verify Result');
    [thinking, firstText, firstTool, secondText].forEach((element, index) => {
      const next = [firstText, firstTool, secondText, secondTool][index];
      expect(element.compareDocumentPosition(next) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });
    expect(screen.getAllByText('The checks passed.')).toHaveLength(1);
  });

  it('keeps an expanded row at its original position as the turn grows and ends', () => {
    const first = message('first', 'assistant', request('read', 'read_file'));
    const { rerender } = render(<Transcript messages={[prompt, first]} active />, {
      wrapper: IntlTestWrapper,
    });
    const trigger = screen.getByRole('button', { name: /1 tool call.*Running/ });
    fireEvent.click(trigger);

    rerender(
      <Transcript
        messages={[
          prompt,
          first,
          message('read-result', 'user', response('read')),
          message('second', 'assistant', request('verify', 'verify_result')),
        ]}
      />
    );
    expect(screen.getByRole('button', { name: /2 tool calls.*Incomplete/ })).toBe(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Verify Result')).toBeVisible();
    fireEvent.click(trigger);
    const assistant = screen.getAllByTestId('message-container').at(-1);
    expect(assistant).toBeDefined();
    expect(
      within(assistant as HTMLElement).getByText(formatMessageTimestamp(first.created))
    ).toBeVisible();
  });

  it('keeps the approval actionable outside the collapsed disclosure', () => {
    render(
      <Transcript
        active
        messages={[
          prompt,
          message('read', 'assistant', request('read', 'read_file')),
          message('read-result', 'user', response('read')),
          message('write', 'assistant', request('write', 'write_file')),
          message('approval', 'assistant', {
            type: 'actionRequired',
            data: {
              actionType: 'toolConfirmation',
              id: 'write',
              generation: 'generation',
              toolName: 'write_file',
              arguments: {},
            },
          }),
        ]}
      />,
      { wrapper: IntlTestWrapper }
    );

    const trigger = screen.getByRole('button', { name: /Waiting for approval/ });
    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    const allow = screen.getByRole('button', { name: 'Allow Once' });
    fireEvent.click(trigger);
    expect(screen.getAllByRole('button', { name: 'Allow Once' })).toEqual([allow]);
    fireEvent.click(allow);
    expect(resolvePermission).toHaveBeenCalledWith('session', 'write', 'generation', 'allow_once');
  });

  it('keeps the same MCP App iframe mounted through expansion and collapse', () => {
    render(
      <Transcript
        messages={[
          prompt,
          message('app', 'assistant', request('draft', 'demo__draft')),
          message('app-result', 'user', response('draft', 'app')),
        ]}
      />,
      { wrapper: IntlTestWrapper }
    );

    const trigger = screen.getByRole('button', { name: /1 tool call.*Completed/ });
    const frame = screen.getByTitle('Draft app');
    expect(screen.queryByText('Draft')).toBeNull();
    fireEvent.click(trigger);
    expect(screen.getByText('Draft')).toBeVisible();
    expect(screen.getByTitle('Draft app')).toBe(frame);
    fireEvent.click(trigger);
    expect(screen.getByTitle('Draft app')).toBe(frame);
  });
});

describe('turn activity state', () => {
  function turns(messages: Message[], active: boolean, showThinking = true) {
    return deriveTurnActivity(
      messages.map((message, index) => ({ message, index })),
      deriveMessageRowContexts(messages),
      isUserMessage,
      active,
      showThinking
    );
  }

  it('tracks running and last tools within the current turn and starts over at a user prompt', () => {
    const call = message('read', 'assistant', request('read', 'developer__read_file'));
    const result = message('result', 'user', response('read'));
    expect(turns([prompt, call], true)[0]).toMatchObject({
      status: 'running',
      runningTool: 'read file',
      lastTool: 'read file',
    });
    expect(turns([prompt, call, result], true)[0]).toMatchObject({
      runningTool: undefined,
      lastTool: 'read file',
    });
    expect(
      turns([prompt, call, result, message('next', 'user', text('Next'))], true)
    ).toMatchObject([
      { status: 'completed', toolCount: 1 },
      { toolCount: 0, runningTool: undefined, lastTool: undefined },
    ]);
  });

  it('distinguishes tool failure from a turn that ended with no result', () => {
    const call = message('read', 'assistant', request('read', 'read_file'));
    expect(turns([prompt, call], false)[0].status).toBe('incomplete');
    expect(
      turns([prompt, call, message('result', 'user', response('read', 'error'))], false)[0].status
    ).toBe('failed');
  });

  it('honors Show Thinking: Never inside turn activity', () => {
    const step = message(
      'step',
      'assistant',
      { type: 'thinking', thinking: 'Private thought', signature: '' },
      request('read', 'read_file')
    );
    const [turn] = turns([prompt, step], false, false);
    expect(turn.steps.map(({ message }) => message.content.map(({ type }) => type))).toEqual([
      ['toolRequest'],
    ]);
  });
});
