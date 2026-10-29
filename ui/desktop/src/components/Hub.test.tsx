/**
 * @vitest-environment jsdom
 */
import { act, render, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MemoryRouter } from 'react-router';
import Hub from './Hub';
import { IntlTestWrapper } from '../i18n/test-utils';
import { createSession } from '../sessions';
import { UserInput } from '../types/message';
import { acpGetLiveVoiceAvailability } from '../acp/liveVoice';
import { subscribeToAcpRecovery } from '../acp/acpConnection';
import type { LiveVoiceController } from '../liveVoice/useLiveVoice';

type ChatInputCapture = {
  draftRef?: { current: string };
  handleSubmit: (input: UserInput) => void;
  workingDir: string;
  onWorkingDirChange: (dir: string) => void;
  liveVoice?: {
    availability: { status: string; message: string } | null;
    start: () => Promise<void>;
  };
  onNextChatExtensionDraftChange?: (draft: { selectedNames: Set<string> }) => void;
};

const liveVoice: LiveVoiceController = {
  activeSessionId: null,
  liveVoiceSessionId: null,
  phase: 'idle',
  muted: false,
  start: vi.fn(),
  stop: vi.fn(),
  toggleMute: vi.fn(),
};

const captured = vi.hoisted(() => ({ chatInput: null as ChatInputCapture | null }));

vi.mock('./ChatInput', () => ({
  default: (props: ChatInputCapture) => {
    captured.chatInput = props;
    return <div data-testid="chat-input" />;
  },
}));

vi.mock('./LoadingGoose', () => ({ default: () => <div /> }));

vi.mock('./ConfigContext', () => ({
  useConfig: () => ({ extensionsList: [] }),
}));

vi.mock('../sessions', () => ({ createSession: vi.fn() }));

vi.mock('../utils/workingDir', () => ({
  getInitialWorkingDir: () => '/tmp/goose',
  getEffectiveWorkingDir: (inheritedWorkingDir?: string) =>
    Promise.resolve(inheritedWorkingDir ?? '/tmp/goose'),
}));

vi.mock('../utils/nextChatExtensions', () => ({
  createNextChatExtensionDraft: () => ({}),
  selectNextChatExtensions: (_list: unknown, draft: { selectedNames: Set<string> }) =>
    [...draft.selectedNames].map((name) => ({ name, type: 'stdio' })),
}));

vi.mock('../acp/errors', () => ({ formatAcpError: (error: unknown) => String(error) }));

vi.mock('../toasts', () => ({ toastError: vi.fn() }));

vi.mock('../acp/liveVoice', () => ({ acpGetLiveVoiceAvailability: vi.fn() }));

vi.mock('../acp/acpConnection', () => ({ subscribeToAcpRecovery: vi.fn() }));

const DRAFT = 'a half-written thought';

function renderHub(draftRef: { current: string }, setView = vi.fn(), initialWorkingDir?: string) {
  return render(
    <IntlTestWrapper>
      <MemoryRouter initialEntries={[{ pathname: '/', state: { workingDir: initialWorkingDir } }]}>
        <Hub setView={setView} draftRef={draftRef} liveVoice={liveVoice} />
      </MemoryRouter>
    </IntlTestWrapper>
  );
}

async function submit() {
  await act(async () => {
    captured.chatInput?.handleSubmit({ msg: DRAFT, images: [] });
  });
}

describe('Hub', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    liveVoice.activeSessionId = null;
    liveVoice.liveVoiceSessionId = null;
    liveVoice.phase = 'idle';
    captured.chatInput = null;
    vi.mocked(acpGetLiveVoiceAvailability).mockRejectedValue(new Error('ACP unavailable'));
    vi.mocked(subscribeToAcpRecovery).mockReturnValue(() => undefined);
  });

  it.each([undefined, '/projects/payments'])(
    'navigates with the inherited folder unless another folder is selected (%s)',
    async (selectedDir) => {
      const setView = vi.fn();
      renderHub({ current: '' }, setView, '/projects/website');

      await waitFor(() => expect(captured.chatInput?.workingDir).toBe('/projects/website'));
      if (selectedDir) {
        await act(async () => captured.chatInput?.onWorkingDirChange(selectedDir));
      }
      await submit();

      expect(createSession).not.toHaveBeenCalled();
      expect(setView).toHaveBeenCalledWith('pair', {
        disableAnimation: true,
        initialMessage: { msg: DRAFT, images: [] },
        workingDir: selectedDir ?? '/projects/website',
      });
    }
  );

  it('requests Live voice availability again after ACP recovers', async () => {
    let recoveryChanged: ((recovering: boolean) => void) | undefined;
    const available = { status: 'ready' as const, message: 'Start Live voice' };
    vi.mocked(acpGetLiveVoiceAvailability)
      .mockRejectedValueOnce(new Error('ACP disconnected'))
      .mockResolvedValueOnce(available);
    vi.mocked(subscribeToAcpRecovery).mockImplementation((listener) => {
      recoveryChanged = listener;
      return () => undefined;
    });

    renderHub({ current: '' });
    await waitFor(() => expect(acpGetLiveVoiceAvailability).toHaveBeenCalledTimes(1));

    act(() => recoveryChanged?.(true));
    act(() => recoveryChanged?.(false));

    await waitFor(() => {
      expect(acpGetLiveVoiceAvailability).toHaveBeenCalledTimes(2);
      expect(captured.chatInput?.liveVoice?.availability).toEqual(available);
    });
  });

  it('returns to the session with the active Live voice interaction', async () => {
    const setView = vi.fn();
    liveVoice.activeSessionId = 'session-with-live-voice';
    renderHub({ current: '' }, setView);

    await act(async () => captured.chatInput?.liveVoice?.start?.());

    expect(setView).toHaveBeenCalledWith('pair', {
      resumeSessionId: 'session-with-live-voice',
    });
    expect(createSession).not.toHaveBeenCalled();
  });

  it('navigates with no extensions when the user cleared the picker', async () => {
    const setView = vi.fn();
    renderHub({ current: '' }, setView);

    // Touching the picker is what turns "not specified" into a real choice, and
    // clearing it is the case the composer already promises in a toast.
    await act(async () => {
      captured.chatInput?.onNextChatExtensionDraftChange?.({ selectedNames: new Set() });
    });
    await submit();

    expect(createSession).not.toHaveBeenCalled();
    expect(setView).toHaveBeenCalledWith('pair', {
      disableAnimation: true,
      initialMessage: { msg: DRAFT, images: [] },
      workingDir: '/tmp/goose',
      extensionConfigs: [],
    });
  });

  it('leaves the set unspecified when the picker was never touched', async () => {
    const setView = vi.fn();
    renderHub({ current: '' }, setView);

    await submit();

    expect(createSession).not.toHaveBeenCalled();
    expect(setView).toHaveBeenCalledWith('pair', {
      disableAnimation: true,
      initialMessage: { msg: DRAFT, images: [] },
      workingDir: '/tmp/goose',
    });
  });

  it('hands the draft to the input', () => {
    const draftRef = { current: DRAFT };
    renderHub(draftRef);

    expect(captured.chatInput?.draftRef).toBe(draftRef);
  });

  it('leaves the draft for Pair to clear after the chat starts', async () => {
    const draftRef = { current: DRAFT };
    renderHub(draftRef);

    await submit();

    expect(draftRef.current).toBe(DRAFT);
    expect(createSession).not.toHaveBeenCalled();
  });
});
