import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../i18n/test-utils';
import { useConfig } from '../ConfigContext';
import AgentLoopSettings from './AgentLoopSettings';

vi.mock('../ConfigContext', () => ({ useConfig: vi.fn() }));

describe('AgentLoopSettings', () => {
  const read = vi.fn();
  const remove = vi.fn();
  const upsert = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(window.electron.getSetting).mockResolvedValue(false);
    read.mockImplementation((key: string) => {
      if (key === 'GOOSE_AUTO_EFFORT_ENABLED') return Promise.resolve(false);
      if (key === 'TYPESAFE_API_KEY') return Promise.resolve({ maskedValue: '********' });
      return Promise.resolve(null);
    });
    vi.mocked(useConfig).mockReturnValue({ read, remove, upsert } as unknown as ReturnType<
      typeof useConfig
    >);
  });

  it('shows and removes a saved TypeSafe API key', async () => {
    render(<AgentLoopSettings />, { wrapper: IntlTestWrapper });

    expect(await screen.findByText('Configured')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Remove key' }));

    await waitFor(() => expect(remove).toHaveBeenCalledWith('TYPESAFE_API_KEY', true));
  });

  it('persists automatic effort enablement', async () => {
    render(<AgentLoopSettings />, { wrapper: IntlTestWrapper });

    fireEvent.click(
      await screen.findByRole('checkbox', { name: 'Enable Automatic thinking effort' })
    );

    await waitFor(() =>
      expect(upsert).toHaveBeenCalledWith('GOOSE_AUTO_EFFORT_ENABLED', true, false)
    );
  });
});
