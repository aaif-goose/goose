import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AppEvents } from '../constants/events';
import {
  acpGetSessionListItem,
  acpListRecentSessions,
  type SessionListItem,
} from '../acp/sessions';

const navigateMock = vi.hoisted(() => vi.fn());
const routerState = vi.hoisted(() => ({ searchParams: new URLSearchParams() }));

vi.mock('react-router', () => ({
  useNavigate: () => navigateMock,
  useLocation: () => ({ pathname: '/pair', search: routerState.searchParams.toString() }),
  useSearchParams: () => [routerState.searchParams, vi.fn()],
}));

vi.mock('../acp/sessions', () => ({
  acpGetSessionListItem: vi.fn(() => new Promise(() => {})),
  acpListRecentSessions: vi.fn().mockResolvedValue([]),
}));

vi.mock('../contexts/ChatContext', () => ({
  useChatContext: () => ({ chat: { sessionId: undefined } }),
}));

import { useNavigationSessions } from './useNavigationSessions';

const injectedSessionId = 'session-1&shouldStartAgent=true';

function expectSingleSessionParameter(target: string) {
  const url = new URL(target, 'http://localhost');
  expect(url.searchParams.get('resumeSessionId')).toBe(injectedSessionId);
  expect(url.searchParams.get('shouldStartAgent')).toBeNull();
}

describe('useNavigationSessions', () => {
  beforeEach(() => {
    navigateMock.mockReset();
    routerState.searchParams = new URLSearchParams();
  });

  it('keeps a selected session ID in one query parameter', () => {
    const { result } = renderHook(() => useNavigationSessions());

    act(() => result.current.handleSessionClick(injectedSessionId));

    expectSingleSessionParameter(navigateMock.mock.calls[0][0]);
  });

  it('keeps a retained session ID in one query parameter', () => {
    routerState.searchParams = new URLSearchParams({ resumeSessionId: injectedSessionId });
    const { result } = renderHook(() => useNavigationSessions());

    act(() => result.current.handleNavClick('/pair'));

    expectSingleSessionParameter(navigateMock.mock.calls[0][0]);
  });
});

function listItem(id: string, name: string, messageCount = 1): SessionListItem {
  return {
    id,
    name,
    workingDir: '/work',
    updatedAt: '2026-11-13T00:00:00Z',
    messageCount,
    createdAt: '2026-11-13T00:00:00Z',
  };
}

function dispatchStatus(sessionId: string, streamState: 'idle' | 'loading' | 'streaming') {
  act(() => {
    window.dispatchEvent(
      new CustomEvent(AppEvents.SESSION_STATUS_UPDATE, {
        detail: { sessionId, streamState, messageCount: 1 },
      })
    );
  });
}

describe('useNavigationSessions session names', () => {
  const listRecent = vi.mocked(acpListRecentSessions);
  const getListItem = vi.mocked(acpGetSessionListItem);

  beforeEach(() => {
    routerState.searchParams = new URLSearchParams();
    listRecent.mockReset();
    listRecent.mockResolvedValue([]);
    getListItem.mockReset();
    getListItem.mockImplementation(() => new Promise(() => {}));
  });

  async function renderWithSessions(sessions: SessionListItem[]) {
    listRecent.mockResolvedValueOnce(sessions);
    const rendered = renderHook(() => useNavigationSessions());
    await act(() => rendered.result.current.fetchSessions());
    return rendered;
  }

  it('reloads the session names when a turn ends', async () => {
    const { result } = await renderWithSessions([listItem('session-1', 'Old name')]);
    listRecent.mockResolvedValue([listItem('session-1', 'Renamed from the CLI')]);

    dispatchStatus('session-1', 'streaming');
    dispatchStatus('session-1', 'idle');

    await waitFor(() => expect(result.current.recentSessions[0].name).toBe('Renamed from the CLI'));
  });

  it('reloads the session names when a chat loads its session again', async () => {
    const { result } = await renderWithSessions([listItem('session-1', 'Old name')]);
    listRecent.mockResolvedValue([listItem('session-1', 'Renamed from the CLI')]);

    dispatchStatus('session-1', 'loading');
    dispatchStatus('session-1', 'idle');

    await waitFor(() => expect(result.current.recentSessions[0].name).toBe('Renamed from the CLI'));
  });

  it('does not reload the session list for other status updates', async () => {
    await renderWithSessions([listItem('session-1', 'Old name')]);
    const callsAfterFirstLoad = listRecent.mock.calls.length;

    dispatchStatus('session-1', 'idle');
    dispatchStatus('session-1', 'idle');
    dispatchStatus('session-1', 'streaming');
    await act(async () => {});

    expect(listRecent.mock.calls.length).toBe(callsAfterFirstLoad);
  });

  it('keeps a rename made while the session names reload', async () => {
    const { result } = await renderWithSessions([listItem('session-1', 'Old name')]);
    let finishReload!: (sessions: SessionListItem[]) => void;
    listRecent.mockImplementationOnce(() => new Promise((resolve) => (finishReload = resolve)));

    dispatchStatus('session-1', 'streaming');
    dispatchStatus('session-1', 'idle');
    act(() => {
      window.dispatchEvent(
        new CustomEvent(AppEvents.SESSION_RENAMED, {
          detail: { sessionId: 'session-1', newName: 'Renamed in Desktop', userInitiated: true },
        })
      );
    });
    await act(async () => finishReload([listItem('session-1', 'Old name')]));

    expect(result.current.recentSessions[0].name).toBe('Renamed in Desktop');
  });

  it('does not bring back a session deleted while the session names reload', async () => {
    const { result } = await renderWithSessions([
      listItem('session-1', 'First'),
      listItem('session-2', 'Second'),
    ]);
    let finishReload!: (sessions: SessionListItem[]) => void;
    listRecent.mockImplementationOnce(() => new Promise((resolve) => (finishReload = resolve)));

    dispatchStatus('session-1', 'streaming');
    dispatchStatus('session-1', 'idle');
    listRecent.mockResolvedValue([listItem('session-1', 'First')]);
    act(() => {
      window.dispatchEvent(
        new CustomEvent(AppEvents.SESSION_DELETED, { detail: { sessionId: 'session-2' } })
      );
    });
    await act(async () => {});
    await act(async () =>
      finishReload([listItem('session-1', 'First'), listItem('session-2', 'Second')])
    );

    expect(result.current.recentSessions.map((session) => session.id)).toEqual(['session-1']);
  });

  it('keeps a new empty session when the session names reload', async () => {
    routerState.searchParams = new URLSearchParams({ resumeSessionId: 'new-session' });
    getListItem.mockResolvedValue(listItem('new-session', '', 0));
    const { result } = await renderWithSessions([listItem('session-1', 'Old name')]);
    await waitFor(() => expect(result.current.recentSessions[0].id).toBe('new-session'));
    listRecent.mockResolvedValue([listItem('session-1', 'Renamed from the CLI')]);

    dispatchStatus('session-1', 'streaming');
    dispatchStatus('session-1', 'idle');

    await waitFor(() =>
      expect(result.current.recentSessions.map((session) => [session.id, session.name])).toEqual([
        ['new-session', ''],
        ['session-1', 'Renamed from the CLI'],
      ])
    );
  });
});
