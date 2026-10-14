/**
 * @vitest-environment jsdom
 */
import { forwardRef, type ReactNode } from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import { ChatState } from '../types/chatState';
import ChatInput from './ChatInput';

const MAX_HEIGHT = 240;
const MIN_HEIGHT = 38;
const LONG_TEXT = Array.from({ length: 40 }, (_, i) => `line ${i}`).join('\n');

type ObserverCallback = (entries: unknown[], observer: unknown) => void;
const resizeCallbacks: ObserverCallback[] = [];

class ResizeObserverStub {
  constructor(callback: ObserverCallback) {
    resizeCallbacks.push(callback);
  }
  observe() {}
  unobserve() {}
  disconnect() {}
}

vi.mock('./icons', () => ({
  Attach: () => null,
  Close: () => null,
  Microphone: () => null,
}));

vi.mock('./ui/Tooltip', () => ({
  Tooltip: ({ children }: { children?: ReactNode }) => <>{children}</>,
  TooltipTrigger: ({ children }: { children?: ReactNode }) => <>{children}</>,
  TooltipContent: () => null,
}));

vi.mock('./MessageQueue', () => ({ MessageQueue: () => null }));
vi.mock('./ui/Diagnostics', () => ({ DiagnosticsModal: () => null }));
vi.mock('./LiveVoiceButton', () => ({ LiveVoiceButton: () => null }));
vi.mock('./MentionPopover', () => ({
  default: forwardRef(function MockMentionPopover() {
    return null;
  }),
}));
vi.mock('./bottom_menu/DirSwitcher', () => ({ DirSwitcher: () => null }));
vi.mock('./bottom_menu/BottomMenuExtensionSelection', () => ({
  BottomMenuExtensionSelection: () => null,
}));
vi.mock('./bottom_menu/CostTracker', () => ({ CostTracker: () => null }));
vi.mock('./bottom_menu/ContextWindowIndicator', () => ({ ContextWindowIndicator: () => null }));
vi.mock('./GitBranchIndicator', () => ({ GitBranchIndicator: () => null }));
vi.mock('./settings/models/bottom_bar/ModelsBottomBar', () => ({ default: () => null }));
vi.mock('./settings/models/predefinedModelsUtils', () => ({ getPredefinedModelsFromEnv: () => [] }));
vi.mock('./alerts', () => ({
  AlertType: { Error: 'error', Warning: 'warning', Info: 'info' },
  useAlerts: () => ({ alerts: [], addAlert: vi.fn(), clearAlerts: vi.fn() }),
}));
vi.mock('./ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({
    getCurrentModelAndProvider: vi
      .fn()
      .mockResolvedValue({ model: 'test-model', provider: 'test-provider' }),
    currentModel: 'test-model',
    currentProvider: 'test-provider',
  }),
}));
vi.mock('../acp/providers', () => ({ acpGetProviderDetails: vi.fn().mockResolvedValue(null) }));
vi.mock('../hooks/useAudioRecorder', () => ({
  useAudioRecorder: () => ({
    isEnabled: false,
    dictationProvider: null,
    isRecording: false,
    isTranscribing: false,
    startRecording: vi.fn(),
    stopRecording: vi.fn(),
  }),
}));
vi.mock('../hooks/useFocusOnTyping', () => ({ useFocusOnTyping: () => {} }));
vi.mock('../toasts', () => ({ toastError: vi.fn() }));
vi.mock('../updates', () => ({ COST_TRACKING_ENABLED: false }));
vi.mock('../utils/workingDir', () => ({ getInitialWorkingDir: () => '/tmp/workspace' }));
vi.mock('../utils/keyboardShortcuts', () => ({ getNavigationShortcutText: () => 'Type a message' }));
vi.mock('../utils/canonical', () => ({ fetchCanonicalModelInfo: vi.fn().mockResolvedValue(null) }));
vi.mock('../utils/analytics', () => ({
  trackFileAttached: vi.fn(),
  trackVoiceDictation: vi.fn(),
  trackDiagnosticsOpened: vi.fn(),
}));
vi.mock('../liveVoice/useLiveVoice', () => ({ isLiveVoiceActive: () => false }));

function renderComposer(handleSubmit = vi.fn()) {
  render(
    <ChatInput
      sessionId="session-1"
      handleSubmit={handleSubmit}
      chatState={ChatState.Idle}
      hasActiveRun={false}
      setView={vi.fn()}
    />,
    { wrapper: IntlTestWrapper }
  );
  const textarea = screen.getByTestId('chat-input') as HTMLTextAreaElement;
  Object.defineProperty(textarea, 'scrollHeight', {
    configurable: true,
    get: () => (textarea.value.length > 0 ? MAX_HEIGHT : MIN_HEIGHT),
  });
  return { handleSubmit, textarea };
}

function fireResizeObservers() {
  act(() => {
    resizeCallbacks.forEach((callback) => callback([], {}));
  });
}

describe('ChatInput empty-composer height', () => {
  beforeEach(() => {
    resizeCallbacks.length = 0;
    vi.stubGlobal('ResizeObserver', ResizeObserverStub);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('collapses the composer after submitting and keeps it collapsed', () => {
    vi.useFakeTimers();
    const { handleSubmit, textarea } = renderComposer();

    fireEvent.change(textarea, { target: { value: LONG_TEXT } });
    act(() => {
      vi.advanceTimersByTime(60);
    });
    expect(textarea.style.height).toBe(`${MAX_HEIGHT}px`);

    const form = textarea.closest('form');
    expect(form).not.toBeNull();
    if (!form) throw new Error('composer form not found');
    fireEvent.submit(form);

    expect(handleSubmit).toHaveBeenCalledTimes(1);
    expect(textarea.value).toBe('');
    expect(textarea.style.height).toBe(`${MIN_HEIGHT}px`);

    // A late resize while the composer is empty must re-assert the minimum,
    // even if something had left the box at its previous grown height.
    textarea.style.height = `${MAX_HEIGHT}px`;
    fireResizeObservers();
    expect(textarea.style.height).toBe(`${MIN_HEIGHT}px`);
  });

  it('never collapses the composer while it still has content', () => {
    const { textarea } = renderComposer();

    textarea.value = 'still typing';
    textarea.style.height = `${MAX_HEIGHT}px`;
    fireResizeObservers();

    expect(textarea.style.height).toBe(`${MAX_HEIGHT}px`);
  });
});
