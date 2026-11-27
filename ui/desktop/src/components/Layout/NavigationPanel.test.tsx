import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { IntlProvider } from 'react-intl';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { acpListRecentSessions } from '../../acp/sessions';
import { Navigation } from './NavigationPanel';

vi.mock('./NavigationContext', () => ({
  useNavigationContext: () => ({ isNavExpanded: true }),
}));
vi.mock('../ConfigContext', () => ({
  useConfig: () => ({ extensionsList: [] }),
}));
vi.mock('../../contexts/ChatContext', () => ({
  useChatContext: () => null,
}));
vi.mock('../../acp/sessions', () => ({
  acpListRecentSessions: vi.fn(),
}));

function renderNavigation() {
  return render(
    <IntlProvider locale="en">
      <MemoryRouter>
        <Navigation activeLiveVoiceSessionId={null} />
      </MemoryRouter>
    </IntlProvider>
  );
}

function chatTitles() {
  return screen.getAllByText(/^(Newest|Middle|Older) chat$/).map((row) => row.textContent);
}

describe('Navigation chat history', () => {
  beforeEach(() => {
    localStorage.clear();
    const timestamps = { messageCount: 1, createdAt: '2026-01-01T00:00:00Z' };
    vi.mocked(acpListRecentSessions).mockResolvedValue([
      {
        ...timestamps,
        id: 'older',
        name: 'Older chat',
        workingDir: '/work/goose/',
        lastMessageAt: '2026-01-02T00:00:00Z',
        updatedAt: '2026-01-05T00:00:00Z',
      },
      {
        ...timestamps,
        id: 'newest',
        name: 'Newest chat',
        workingDir: '/work/goose',
        updatedAt: '2026-01-04T00:00:00Z',
      },
      {
        ...timestamps,
        id: 'middle',
        name: 'Middle chat',
        workingDir: '/forks/goose',
        updatedAt: '2026-01-03T00:00:00Z',
      },
    ]);
  });

  it('switches from project groups to a timeline and remembers the choice', async () => {
    const user = userEvent.setup();
    const { unmount } = renderNavigation();
    await screen.findByText('Newest chat');

    expect(screen.getByRole('button', { name: 'work/goose' })).toBeInTheDocument();
    expect(chatTitles()).toEqual(['Newest chat', 'Older chat', 'Middle chat']);

    await user.click(screen.getByRole('button', { name: 'Chat history view' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Most recent first' }));

    expect(chatTitles()).toEqual(['Newest chat', 'Middle chat', 'Older chat']);
    expect(screen.queryByRole('button', { name: 'work/goose' })).not.toBeInTheDocument();
    expect(screen.getAllByText('work/goose')).toHaveLength(2);
    expect(screen.getByText('forks/goose')).toBeInTheDocument();

    unmount();
    renderNavigation();
    await screen.findByText('Newest chat');
    expect(chatTitles()).toEqual(['Newest chat', 'Middle chat', 'Older chat']);

    await user.click(screen.getByRole('button', { name: 'Chat history view' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Group by project' }));
    expect(chatTitles()).toEqual(['Newest chat', 'Older chat', 'Middle chat']);
  });
});
