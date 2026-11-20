import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, onTestFinished, vi } from 'vitest';
import { ShowThinkingSetting } from './ShowThinkingSetting';
import { AppEvents } from '../../../constants/events';
import { IntlTestWrapper } from '../../../i18n/test-utils';

describe('ShowThinkingSetting', () => {
  afterEach(() => window.electron.setSetting('showThinking', 'collapsed'));

  it('saves the chosen option and notifies open chats', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    window.addEventListener(AppEvents.SHOW_THINKING_CHANGED, onChange);
    onTestFinished(() => window.removeEventListener(AppEvents.SHOW_THINKING_CHANGED, onChange));
    render(<ShowThinkingSetting />, { wrapper: IntlTestWrapper });

    await user.click(await screen.findByRole('button', { name: 'Collapsed' }));
    await user.click(await screen.findByRole('menuitemradio', { name: 'Never' }));

    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
    expect(window.electron.setSetting).toHaveBeenLastCalledWith('showThinking', 'never');
    expect(screen.getByRole('button', { name: 'Never' })).toBeInTheDocument();
  });

  it('shows the saved option', async () => {
    await window.electron.setSetting('showThinking', 'always');
    render(<ShowThinkingSetting />, { wrapper: IntlTestWrapper });

    expect(await screen.findByRole('button', { name: 'Always' })).toBeInTheDocument();
  });

  it('shows Collapsed for a value it does not know', async () => {
    vi.mocked(window.electron.getSetting).mockResolvedValueOnce(false);
    await act(async () => {
      render(<ShowThinkingSetting />, { wrapper: IntlTestWrapper });
    });

    expect(screen.getByRole('button', { name: 'Collapsed' })).toBeInTheDocument();
  });
});
