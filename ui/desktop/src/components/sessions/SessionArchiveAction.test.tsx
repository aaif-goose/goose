import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { IntlProvider } from 'react-intl';
import { expect, it, vi } from 'vitest';
import { acpSetSessionArchived, type SessionListItem } from '../../acp/sessions';
import { AppEvents } from '../../constants/events';
import { SessionArchiveAction } from './SessionArchiveAction';

vi.mock('../../acp/sessions', () => ({
  acpSetSessionArchived: vi.fn().mockResolvedValue(undefined),
}));

const session: SessionListItem = {
  id: 'session-1',
  name: 'Test chat',
  workingDir: '/work',
  messageCount: 1,
  updatedAt: '2026-11-26T08:00:00Z',
  createdAt: '2026-11-26T08:00:00Z',
};

it('archives a chat through the keyboard and restores it from the history action', async () => {
  const user = userEvent.setup();
  const dispatchEvent = vi.spyOn(window, 'dispatchEvent');
  try {
    const { rerender } = render(
      <IntlProvider locale="en">
        <SessionArchiveAction session={session} disabled={false} />
      </IntlProvider>
    );

    await user.tab();
    expect(screen.getByRole('button', { name: 'Chat actions' })).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(await screen.findByRole('menuitem', { name: 'Archive chat' })).toHaveFocus();
    await user.keyboard('{Enter}');
    await waitFor(() =>
      expect(dispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: AppEvents.SESSION_ARCHIVE_CHANGED,
          detail: { sessionId: session.id, archived: true },
        })
      )
    );
    expect(acpSetSessionArchived).toHaveBeenCalledWith(session.id, true);

    rerender(
      <IntlProvider locale="en">
        <SessionArchiveAction
          session={{ ...session, archivedAt: '2026-11-26T09:00:00Z' }}
          disabled={false}
        />
      </IntlProvider>
    );
    expect(screen.getByText('Archived')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Restore chat' }));
    await waitFor(() =>
      expect(dispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: AppEvents.SESSION_ARCHIVE_CHANGED,
          detail: { sessionId: session.id, archived: false },
        })
      )
    );
    expect(acpSetSessionArchived).toHaveBeenCalledWith(session.id, false);
  } finally {
    dispatchEvent.mockRestore();
  }
});
