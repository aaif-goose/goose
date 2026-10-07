import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import type { Message } from '../types/message';
import GooseMessage from './GooseMessage';

describe('GooseMessage', () => {
  it('renders operation details for a thinking-only message', () => {
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

    expect(screen.getByText('Thinking')).toBeInTheDocument();
    expect(screen.getByLabelText('ops_auto_effort: thinking high')).toBeInTheDocument();
  });

  it('renders operation details for an empty output-limit fallback', () => {
    const message: Message = {
      content: [],
      created: 1,
      metadata: {
        agentVisible: true,
        fallbackContent: true,
        operationLogs: ['ops_auto_effort: thinking high'],
        outputTokenLimitReached: true,
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

    expect(
      screen.getByText("Response reached the model's output-token limit before returning content.")
    ).toBeInTheDocument();
    expect(screen.getByLabelText('ops_auto_effort: thinking high')).toBeInTheDocument();
  });
});
