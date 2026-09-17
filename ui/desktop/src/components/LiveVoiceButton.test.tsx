import { render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import { LiveVoiceButton } from './LiveVoiceButton';

const baseProps: ComponentProps<typeof LiveVoiceButton> = {
  availability: { status: 'ready', message: 'Start Live voice' },
  composerEmpty: true,
  phase: 'idle',
  muted: false,
  activeInAnotherSession: false,
  onStart: vi.fn(),
  onStop: vi.fn(),
  onToggleMute: vi.fn(),
};

function renderButton(props: Partial<typeof baseProps> = {}) {
  return render(<LiveVoiceButton {...baseProps} {...props} />, { wrapper: IntlTestWrapper });
}

describe('LiveVoiceButton', () => {
  it('shows an eligible state for an empty composer', () => {
    renderButton();

    const button = screen.getByTestId('live-voice-button');
    expect(button).toBeEnabled();
    expect(button).toHaveAttribute('data-eligible', 'true');
    expect(button).toHaveAccessibleName('Start Live voice');
  });

  it.each(['idle', 'error'] as const)('shows the unavailable reason while %s', (phase) => {
    renderButton({
      availability: {
        status: 'unavailable',
        message: 'Live voice requires Autonomous mode',
      },
      phase,
    });

    const button = screen.getByTestId('live-voice-button');
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('data-eligible', 'false');
    expect(button).toHaveAccessibleName('Live voice requires Autonomous mode');
  });

  it.each(['idle', 'error'] as const)('applies empty-composer gate while %s', (phase) => {
    renderButton({ composerEmpty: false, phase });

    const button = screen.getByTestId('live-voice-button');
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('data-eligible', 'false');
    expect(button).toHaveAccessibleName('Clear the message and attachments to use Live voice');
  });

  it('does not render without eligibility for the displayed session', () => {
    renderButton({ availability: null });
    expect(screen.queryByTestId('live-voice-button')).not.toBeInTheDocument();
  });

  it('returns to an interaction active in another session', () => {
    const onStart = vi.fn();
    renderButton({
      availability: null,
      composerEmpty: false,
      activeInAnotherSession: true,
      onStart,
    });

    const button = screen.getByTestId('live-voice-button');
    expect(button).toBeEnabled();
    expect(button).toHaveAccessibleName('Return to active Live voice');
    button.click();
    expect(onStart).toHaveBeenCalledOnce();
  });

  it('starts when idle and keeps stop controls available while availability refreshes', () => {
    const onStart = vi.fn();
    const onStop = vi.fn();
    const activeProps = { ...baseProps, availability: null, onStart, onStop };
    const { container, rerender } = renderButton({ onStart, onStop });

    screen.getByTestId('live-voice-button').click();
    expect(onStart).toHaveBeenCalledOnce();

    rerender(<LiveVoiceButton {...activeProps} phase="connecting" />);
    expect(container.querySelector('.animate-spin')).toBeInTheDocument();
    screen.getByTestId('live-voice-button').click();
    expect(onStop).toHaveBeenCalledOnce();

    rerender(<LiveVoiceButton {...activeProps} phase="live" />);
    screen.getByTestId('live-voice-button').click();
    expect(onStop).toHaveBeenCalledTimes(2);
  });

  it('disables the stopping state', () => {
    renderButton({ availability: null, phase: 'stopping' });
    expect(screen.getByTestId('live-voice-button')).toBeDisabled();
  });

  it('toggles microphone mute while live', () => {
    const onToggleMute = vi.fn();
    const liveProps = { availability: null, phase: 'live', onToggleMute } as const;
    const { rerender } = renderButton(liveProps);

    screen.getByTestId('live-voice-mute-button').click();
    expect(onToggleMute).toHaveBeenCalledOnce();
    expect(screen.getByTestId('live-voice-mute-button')).toHaveAccessibleName('Mute microphone');

    rerender(<LiveVoiceButton {...baseProps} {...liveProps} muted />);
    expect(screen.getByTestId('live-voice-mute-button')).toHaveAccessibleName('Unmute microphone');
  });
});
