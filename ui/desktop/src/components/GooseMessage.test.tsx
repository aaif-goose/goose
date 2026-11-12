import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import type { Message } from '../types/message';
import GooseMessage from './GooseMessage';
import type { ToolRenderState } from './messageRowContext';

describe('GooseMessage', () => {
  it('renders operation details for a thinking-only message', async () => {
    const message: Message = {
      content: [{ type: 'thinking', thinking: 'Working it out', signature: '' }],
      created: 1,
      metadata: {
        agentVisible: true,
        operationLogs: ['ops_auto_effort: thinking high'],
        userVisible: true,
      },
      role: 'assistant',
    };

    render(
      <GooseMessage
        sessionId="session"
        message={message}
        hideTimestamp={false}
        toolStates={[]}
        toolNotifications={[]}
        toolConfirmationShownInline={false}
        append={vi.fn()}
        isStreaming={false}
      />,
      { wrapper: IntlTestWrapper }
    );

    expect(await screen.findByText('Thinking')).toBeInTheDocument();
    expect(screen.getByLabelText('ops_auto_effort: thinking high')).toBeInTheDocument();
  });

  function renderFinalOutputTurn(response: ToolRenderState['response']) {
    const message: Message = {
      id: 'assistant-1',
      content: [
        {
          type: 'toolRequest',
          id: 'final-1',
          toolCall: {
            status: 'success',
            value: { name: 'recipe__final_output', arguments: { answer: 'done' } },
          },
        },
      ],
      created: 1,
      metadata: {
        agentVisible: true,
        userVisible: true,
        usage: { inputTokens: 1200, outputTokens: 340, totalTokens: 1540 },
      },
      role: 'assistant',
    };

    render(
      <GooseMessage
        sessionId="session"
        message={message}
        hideTimestamp={false}
        toolStates={[{ requestId: 'final-1', response, confirmation: undefined, isPending: false }]}
        toolNotifications={[undefined]}
        toolConfirmationShownInline={false}
        append={vi.fn()}
        isStreaming={false}
      />,
      { wrapper: IntlTestWrapper }
    );
  }

  it('renders a recipe final output call as a plain line with its usage', () => {
    renderFinalOutputTurn({
      type: 'toolResponse',
      id: 'final-1',
      toolResult: {
        status: 'success',
        value: {
          content: [{ type: 'text', text: 'Final output successfully collected.' }],
          isError: false,
        },
      },
    });

    expect(screen.getByText('Returned structured result')).toBeInTheDocument();
    expect(screen.queryByText(/final output/i)).not.toBeInTheDocument();
    expect(screen.getByText('1.5k tok')).toBeInTheDocument();
  });

  it('reports a rejected final output result', () => {
    renderFinalOutputTurn({
      type: 'toolResponse',
      id: 'final-1',
      toolResult: { status: 'error', error: 'Output does not match the schema' },
    });

    expect(screen.getByText('Structured result was rejected')).toBeInTheDocument();
  });

  it('does not report success for a stopped final output call without a result', () => {
    renderFinalOutputTurn(undefined);

    expect(screen.getByText("Structured result wasn't collected")).toBeInTheDocument();
    expect(screen.queryByText('Returned structured result')).not.toBeInTheDocument();
  });
});
