import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import type { Message } from '../types/message';
import UserMessage from './UserMessage';

const message: Message = {
  content: [{ type: 'text', text: 'Find all callers of load_session' }],
  created: Date.now(),
  id: 'msg-1',
  metadata: { agentVisible: true, userVisible: true },
  role: 'user',
};

describe('UserMessage', () => {
  it('offers editing when the message can be updated', () => {
    render(<UserMessage message={message} onMessageUpdate={vi.fn()} />, {
      wrapper: IntlTestWrapper,
    });

    expect(screen.getByRole('button', { name: /Edit message/ })).toBeInTheDocument();
  });

  it('hides editing when the message cannot be updated', () => {
    render(<UserMessage message={message} />, { wrapper: IntlTestWrapper });

    expect(screen.queryByRole('button', { name: /Edit message/ })).not.toBeInTheDocument();
  });
});
