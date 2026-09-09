import { describe, expect, it, vi, beforeEach } from 'vitest';
import { showExtensionLoadResults } from './extensionErrorUtils';

vi.mock('../toasts', () => ({
  toastService: {
    error: vi.fn(),
    extensionLoading: vi.fn(),
    dismiss: vi.fn(),
  },
}));

import { toastService } from '../toasts';

describe('showExtensionLoadResults', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows nothing when results are empty', () => {
    showExtensionLoadResults([]);
    expect(toastService.error).not.toHaveBeenCalled();
    expect(toastService.extensionLoading).not.toHaveBeenCalled();
  });

  it('dismisses stale toast when all extensions succeed', () => {
    showExtensionLoadResults([
      { name: 'ext-a', success: true },
      { name: 'ext-b', success: true },
    ]);
    expect(toastService.dismiss).toHaveBeenCalledWith('extension-loading');
    expect(toastService.error).not.toHaveBeenCalled();
    expect(toastService.extensionLoading).not.toHaveBeenCalled();
  });

  it('shows individual error toast for a single failed extension', () => {
    showExtensionLoadResults([{ name: 'ext-a', success: false, error: 'connection refused' }]);
    expect(toastService.error).toHaveBeenCalledOnce();
    expect(toastService.extensionLoading).not.toHaveBeenCalled();
  });

  it('shows grouped toast when multiple extensions load and at least one fails', () => {
    showExtensionLoadResults([
      { name: 'ext-a', success: true },
      { name: 'ext-b', success: false, error: 'timeout' },
    ]);
    expect(toastService.extensionLoading).toHaveBeenCalledOnce();
    expect(toastService.error).not.toHaveBeenCalled();
  });
});
