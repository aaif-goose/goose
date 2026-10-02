/**
 * @vitest-environment jsdom
 */
import { act, render, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import Hub from './Hub';
import { IntlTestWrapper } from '../i18n/test-utils';
import { createSession } from '../sessions';
import { UserInput } from '../types/message';
import { acpGetLiveVoiceAvailability } from '../acp/liveVoice';
import { subscribeToAcpRecovery } from '../acp/acpConnection';
import type { LiveVoiceController } from '../liveVoice/useLiveVoice';

type ChatInputCapture = {
  draftRef?: { current: { msg: string; images: unknown[] } };
  handleSubmit: (input: UserInput) => void;
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
  getEffectiveWorkingDir: () => Promise.resolve('/tmp/goose'),
}));

vi.mock('../utils/nextChatExtensions', () => ({
  createNextChatExtensionDraft: () => ({}),
  selectNextChatExtensions: () => [],
}));

vi.mock('../acp/errors', () => ({ formatAcpError: (error: unknown) => String(error) }));

vi.mock('../toasts', () => ({ toastError: vi.fn() }));

vi.mock('../acp/liveVoice', () => ({ acpGetLiveVoiceAvailability: vi.fn() }));

vi.mock('../acp/acpConnection', () => ({ subscribeToAcpRecovery: vi.fn() }));

const DRAFT = 'a half-written thought';

function renderHub(draftRef: { current: { msg: string; images: never[] } }, setView = vi.fn()) {
  return render(
    <IntlTestWrapper>
      <Hub setView={setView} draftRef={draftRef} liveVoice={liveVoice} />
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

    renderHub({ current: { msg: '', images: [] } });
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
    renderHub({ current: { msg: '', images: [] } }, setView);

    await act(async () => captured.chatInput?.liveVoice?.start?.());

    expect(setView).toHaveBeenCalledWith('pair', {
      resumeSessionId: 'session-with-live-voice',
    });
    expect(createSession).not.toHaveBeenCalled();
  });

  it('navigates immediately and leaves session creation to Pair', async () => {
    const setView = vi.fn();
    const draftRef = { current: { msg: DRAFT, images: [] as never[] } };
    renderHub(draftRef, setView);

    await submit();

    expect(createSession).not.toHaveBeenCalled();
    expect(setView).toHaveBeenCalledWith('pair', {
      disableAnimation: true,
      initialMessage: { msg: DRAFT, images: [] },
      workingDir: undefined,
      userSelectedWorkingDir: false,
      allExtensions: [],
    });
    expect(draftRef.current.msg).toBe(DRAFT);
  });

  it('passes a cleared extension picker through as an explicit empty set', async () => {
    const setView = vi.fn();
    renderHub({ current: { msg: '', images: [] } }, setView);

    // Touching the picker is what turns "not specified" into a real choice, and
    // clearing it is the case the composer already promises in a toast.
    await act(async () => {
      captured.chatInput?.onNextChatExtensionDraftChange?.({ selectedNames: new Set() });
    });
    await submit();

    expect(setView).toHaveBeenCalledWith(
      'pair',
      expect.objectContaining({
        extensionConfigs: [],
        userCustomizedExtensions: true,
      })
    );
    expect(createSession).not.toHaveBeenCalled();
  });

  it('hands the draft to the input', () => {
    const draftRef = { current: { msg: DRAFT, images: [] as never[] } };
    renderHub(draftRef);

    expect(captured.chatInput?.draftRef).toBe(draftRef);
  });
});
