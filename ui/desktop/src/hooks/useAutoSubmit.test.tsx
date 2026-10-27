import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, renderHook } from '@testing-library/react';
import { MemoryRouter, useLocation, useNavigate, type NavigateFunction } from 'react-router';
import type { PropsWithChildren } from 'react';
import { useAutoSubmit } from './useAutoSubmit';
import { ChatState } from '../types/chatState';
import { createUserMessage, type UserInput } from '../types/message';
import type { Session } from '../types/session';

function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 'sess-1',
    name: 'untitled',
    message_count: 0,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    working_dir: '/tmp',
    extension_data: { active: [], installed: [] },
    ...overrides,
  } as Session;
}

const initialMessage: UserInput = {
  msg: 'Run the recipe',
  images: [],
};

type AutoSubmitProps = Parameters<typeof useAutoSubmit>[0];

const forkedChatUrl = '/pair?resumeSessionId=sess-1&shouldStartAgent=true';

function AutoSubmitChat(props: AutoSubmitProps) {
  useAutoSubmit(props);
  return null;
}

interface RouterHandle {
  navigate: NavigateFunction;
  search: string;
}

// AppLayout renders the chats under a different parent while Settings is open,
// so they unmount there and mount again on the way back.
function renderChatWithSettingsRoute(props: AutoSubmitProps) {
  const router = {} as RouterHandle;

  function ChatRoute({ chatProps }: { chatProps: AutoSubmitProps }) {
    const location = useLocation();
    router.navigate = useNavigate();
    router.search = location.search;
    return location.pathname === '/pair' ? <AutoSubmitChat {...chatProps} /> : null;
  }

  const view = render(
    <MemoryRouter initialEntries={[forkedChatUrl]}>
      <ChatRoute chatProps={props} />
    </MemoryRouter>
  );

  const rerender = (nextProps: AutoSubmitProps) =>
    view.rerender(
      <MemoryRouter initialEntries={[forkedChatUrl]}>
        <ChatRoute chatProps={nextProps} />
      </MemoryRouter>
    );

  return { router, rerender };
}

function openSettingsAndGoBack(router: RouterHandle) {
  act(() => {
    void router.navigate('/settings');
  });
  act(() => {
    void router.navigate(-1);
  });
}

describe('useAutoSubmit', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('does not auto-submit while recipe acceptance is unresolved', () => {
    const handleSubmit = vi.fn();
    const dispatchEventSpy = vi.spyOn(window, 'dispatchEvent');

    const wrapper = ({ children }: PropsWithChildren) => (
      <MemoryRouter initialEntries={['/pair?resumeSessionId=sess-1']}>{children}</MemoryRouter>
    );

    renderHook(
      () =>
        useAutoSubmit({
          sessionId: 'sess-1',
          session: makeSession(),
          messages: [],
          chatState: ChatState.Idle,
          initialMessage,
          canAutoSubmit: false,
          handleSubmit,
        }),
      { wrapper }
    );

    expect(handleSubmit).not.toHaveBeenCalled();
    expect(dispatchEventSpy).not.toHaveBeenCalled();
  });

  it('keeps the initial message while blocked and submits it once unblocked', () => {
    const handleSubmit = vi.fn();
    const dispatchEventSpy = vi.spyOn(window, 'dispatchEvent');

    const wrapper = ({ children }: PropsWithChildren) => (
      <MemoryRouter initialEntries={['/pair?resumeSessionId=sess-1']}>{children}</MemoryRouter>
    );

    const { rerender } = renderHook(
      ({ canAutoSubmit }) =>
        useAutoSubmit({
          sessionId: 'sess-1',
          session: makeSession(),
          messages: [],
          chatState: ChatState.Idle,
          initialMessage,
          canAutoSubmit,
          handleSubmit,
        }),
      {
        initialProps: { canAutoSubmit: false },
        wrapper,
      }
    );

    expect(handleSubmit).not.toHaveBeenCalled();
    expect(dispatchEventSpy).not.toHaveBeenCalled();

    rerender({ canAutoSubmit: true });

    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(handleSubmit).toHaveBeenCalledWith(initialMessage);
    expect(dispatchEventSpy).toHaveBeenCalledTimes(1);
  });

  it('resumes once and does not resume again after Settings and Back', () => {
    const handleSubmit = vi.fn();
    const { router } = renderChatWithSettingsRoute({
      sessionId: 'sess-1',
      session: makeSession({ message_count: 2 }),
      messages: [],
      chatState: ChatState.Idle,
      initialMessage: undefined,
      canAutoSubmit: true,
      handleSubmit,
    });

    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(handleSubmit).toHaveBeenCalledWith({ msg: '', images: [] });

    openSettingsAndGoBack(router);

    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(router.search).toBe('?resumeSessionId=sess-1');
  });

  it('sends the edited message of a fork once, also after Settings and Back', () => {
    const handleSubmit = vi.fn();
    const props: AutoSubmitProps = {
      sessionId: 'sess-1',
      session: makeSession({ message_count: 1 }),
      messages: [createUserMessage('Earlier question')],
      chatState: ChatState.Idle,
      initialMessage,
      canAutoSubmit: true,
      handleSubmit,
    };
    const { router, rerender } = renderChatWithSettingsRoute(props);

    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(handleSubmit).toHaveBeenCalledWith(initialMessage);

    rerender({ ...props, initialMessage: undefined });
    openSettingsAndGoBack(router);

    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(router.search).toBe('?resumeSessionId=sess-1');
  });

  it('sends the edited first message of a fork once, also after Settings and Back', () => {
    // Editing the first message forks an empty session, so the new-session branch sends it.
    const handleSubmit = vi.fn();
    const props: AutoSubmitProps = {
      sessionId: 'sess-1',
      session: makeSession({ message_count: 0 }),
      messages: [],
      chatState: ChatState.Idle,
      initialMessage,
      canAutoSubmit: true,
      handleSubmit,
    };
    const { router, rerender } = renderChatWithSettingsRoute(props);

    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(handleSubmit).toHaveBeenCalledWith(initialMessage);

    rerender({ ...props, session: makeSession({ message_count: 2 }), initialMessage: undefined });
    openSettingsAndGoBack(router);

    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(router.search).toBe('?resumeSessionId=sess-1');
  });
});
