/**
 * @vitest-environment jsdom
 */
import { fireEvent, render, screen } from '@testing-library/react';
import { Link, MemoryRouter, Route, Routes } from 'react-router';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../i18n/test-utils';
import type { LiveVoiceController } from '../../liveVoice/useLiveVoice';
import { AppLayout } from './AppLayout';

const chatMounts = vi.hoisted(() => vi.fn());

vi.mock('../../contexts/ChatContext', () => ({
  useChatContext: () => ({ setChat: vi.fn() }),
}));

vi.mock('./NavigationPanel', () => ({ Navigation: () => null }));

vi.mock('../ChatSessionsContainer', async () => {
  const { useEffect, useState } = await import('react');
  function ChatSessionsContainerMock() {
    const [draft, setDraft] = useState('');
    useEffect(() => chatMounts(), []);
    return (
      <textarea
        data-testid="chat-draft"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
      />
    );
  }
  return { default: ChatSessionsContainerMock };
});

describe('AppLayout', () => {
  it('keeps the chat mounted when visiting settings and returning', () => {
    chatMounts.mockClear();
    render(
      <IntlTestWrapper>
        <MemoryRouter initialEntries={['/pair']}>
          <Routes>
            <Route
              path="/"
              element={
                <AppLayout
                  activeSessions={[{ sessionId: 'session-1' }]}
                  liveVoice={{ activeSessionId: null } as LiveVoiceController}
                />
              }
            >
              <Route path="pair" element={<Link to="/settings">Settings</Link>} />
              <Route path="settings" element={<Link to="/pair">Back to chat</Link>} />
            </Route>
          </Routes>
        </MemoryRouter>
      </IntlTestWrapper>
    );

    fireEvent.change(screen.getByTestId('chat-draft'), { target: { value: 'unsent message' } });
    fireEvent.click(screen.getByRole('link', { name: 'Settings' }));
    fireEvent.click(screen.getByRole('link', { name: 'Back to chat' }));

    expect(screen.getByTestId('chat-draft')).toHaveValue('unsent message');
    expect(chatMounts).toHaveBeenCalledTimes(1);
  });
});
