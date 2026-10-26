import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import UpdateSection from './UpdateSection';
import { IntlTestWrapper } from '../../../i18n/test-utils';

type UpdaterCallback = Parameters<typeof window.electron.onUpdaterEvent>[0];

let sendUpdaterEvent: UpdaterCallback;

const renderDownloadedUpdate = async () => {
  render(<UpdateSection />, { wrapper: IntlTestWrapper });
  await act(async () => {
    sendUpdaterEvent({ event: 'update-downloaded', data: { version: '1.52.0' } });
  });
  return screen.getByRole('button', { name: 'Install & Restart' });
};

describe('UpdateSection install', () => {
  beforeEach(() => {
    window.electron.getVersion = vi.fn(() => '1.51.0');
    window.electron.getUpdateState = vi.fn().mockResolvedValue(null);
    window.electron.getAutoDownloadDisabled = vi.fn().mockResolvedValue(false);
    window.electron.onUpdaterEvent = vi.fn((callback: UpdaterCallback) => {
      sendUpdaterEvent = callback;
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows why the install failed and brings the button back', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    window.electron.installUpdate = vi.fn().mockResolvedValue({
      success: false,
      error: 'Refusing to auto-update: C:\\Goose does not look like an app install directory',
    });
    const button = await renderDownloadedUpdate();

    await user.click(button);

    expect(await screen.findByText(/does not look like an app install directory/)).toBeVisible();

    act(() => {
      vi.advanceTimersByTime(5000);
    });

    expect(screen.queryByText(/does not look like an app install directory/)).toBeNull();
    expect(screen.getByRole('button', { name: 'Install & Restart' })).toBeEnabled();
  });

  it('disables the button while the install runs', async () => {
    window.electron.installUpdate = vi.fn(() => new Promise<never>(() => {}));
    const user = userEvent.setup();
    const button = await renderDownloadedUpdate();

    await user.click(button);
    await user.click(button);

    expect(button).toBeDisabled();
    expect(window.electron.installUpdate).toHaveBeenCalledTimes(1);
  });
});
