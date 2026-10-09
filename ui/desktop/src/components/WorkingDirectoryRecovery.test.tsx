import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import { WorkingDirectoryRecovery } from './WorkingDirectoryRecovery';

const missing = { reason: 'working_directory_missing' as const, path: '/deleted/project' };

describe('WorkingDirectoryRecovery', () => {
  beforeEach(() => {
    window.electron.directoryChooser = vi.fn();
  });

  it('explains the missing path and retries with the chosen directory', async () => {
    vi.mocked(window.electron.directoryChooser).mockResolvedValue({
      canceled: false,
      filePaths: ['/renamed'],
    });
    const onReplace = vi.fn().mockResolvedValue(undefined);
    render(<WorkingDirectoryRecovery error={missing} onReplace={onReplace} />, {
      wrapper: IntlTestWrapper,
    });

    expect(screen.getByText(/The working directory no longer exists/)).toHaveTextContent(
      missing.path
    );
    fireEvent.click(screen.getByRole('button', { name: 'Choose directory…' }));

    await waitFor(() => expect(onReplace).toHaveBeenCalledWith('/renamed'));
  });

  it.each([
    { canceled: true, filePaths: ['/ignored'] },
    { canceled: false, filePaths: [] },
  ])('keeps the recovery controls after cancellation or an empty selection', async (result) => {
    vi.mocked(window.electron.directoryChooser).mockResolvedValue(result);
    const onReplace = vi.fn();
    render(<WorkingDirectoryRecovery error={missing} onReplace={onReplace} />, {
      wrapper: IntlTestWrapper,
    });
    const button = screen.getByRole('button', { name: 'Choose directory…' });

    fireEvent.click(button);

    await waitFor(() => expect(button).toBeEnabled());
    expect(onReplace).not.toHaveBeenCalled();
    vi.mocked(window.electron.directoryChooser).mockResolvedValue({
      canceled: false,
      filePaths: ['/replacement'],
    });
    fireEvent.click(button);
    await waitFor(() => expect(onReplace).toHaveBeenCalledWith('/replacement'));
  });

  it('allows a server path to be entered without changing Windows separators', async () => {
    const onReplace = vi.fn().mockResolvedValue(undefined);
    const path = '\\\\remote\\share\\project';
    render(
      <WorkingDirectoryRecovery
        error={{ ...missing, recoveryError: 'invalid directory path' }}
        onReplace={onReplace}
      />,
      { wrapper: IntlTestWrapper }
    );
    expect(screen.getByRole('alert')).toHaveTextContent('invalid directory path');

    fireEvent.change(screen.getByRole('textbox', { name: 'Enter path' }), {
      target: { value: path },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));

    await waitFor(() => expect(onReplace).toHaveBeenCalledWith(path));
    expect(window.electron.directoryChooser).not.toHaveBeenCalled();
  });

  it('allows only one directory picker at a time and reports picker failures', async () => {
    let reject!: (error: Error) => void;
    vi.mocked(window.electron.directoryChooser).mockReturnValue(
      new Promise((_, rejectPromise) => {
        reject = rejectPromise;
      })
    );
    const onReplace = vi.fn();
    render(<WorkingDirectoryRecovery error={missing} onReplace={onReplace} />, {
      wrapper: IntlTestWrapper,
    });
    const button = screen.getByRole('button', { name: 'Choose directory…' });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(window.electron.directoryChooser).toHaveBeenCalledTimes(1);
    expect(button).toBeDisabled();

    reject(new Error('Picker unavailable'));

    await waitFor(() => expect(button).toBeEnabled());
    expect(screen.getByRole('alert')).toHaveTextContent('Picker unavailable');
    expect(onReplace).not.toHaveBeenCalled();
  });
});
