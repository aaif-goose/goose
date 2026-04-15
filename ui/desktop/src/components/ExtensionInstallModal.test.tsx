/* eslint-disable @typescript-eslint/no-explicit-any */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, type RenderOptions, screen, act } from '@testing-library/react';
import { ExtensionInstallModal } from './ExtensionInstallModal';
import { addExtensionFromDeepLink } from './settings/extensions/deeplink';
import { IntlTestWrapper } from '../i18n/test-utils';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

vi.mock('./settings/extensions/deeplink', () => ({
  addExtensionFromDeepLink: vi.fn(),
}));

const mockElectron = {
  getConfig: vi.fn(),
  getAllowedExtensions: vi.fn(),
  logInfo: vi.fn(),
  on: vi.fn(),
  off: vi.fn(),
};

(window as any).electron = mockElectron;

vi.mock('./ConfigContext', () => ({
  useConfig: () => ({
    extensionsList: [],
    getExtensions: vi.fn().mockResolvedValue([]),
  }),
}));

describe('ExtensionInstallModal', () => {
  const mockAddExtension = vi.fn();
  const mockSetView = vi.fn();

  const getAddExtensionEventHandler = () => {
    const addExtensionCall = mockElectron.on.mock.calls.find((call) => call[0] === 'add-extension');
    expect(addExtensionCall).toBeDefined();
    return addExtensionCall![1];
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockElectron.getConfig.mockReturnValue({
      GOOSE_ALLOWLIST_WARNING: false,
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('Extension Request Handling', () => {
    it('should handle trusted extension (default behaviour, no allowlist)', async () => {
      mockElectron.getAllowedExtensions.mockResolvedValue([]);

      renderWithIntl(<ExtensionInstallModal addExtension={mockAddExtension} setView={mockSetView} />);

      const eventHandler = getAddExtensionEventHandler();

      await act(async () => {
        await eventHandler({}, 'goose://extension?cmd=npx&arg=test-extension&name=TestExt');
      });

      expect(screen.getByRole('dialog')).toBeInTheDocument();
      expect(screen.getByText('Confirm Extension Installation')).toBeInTheDocument();
      expect(screen.getByText(/TestExt extension/)).toBeInTheDocument();
      expect(screen.getAllByRole('button')).toHaveLength(3);
    });

    it('should handle trusted extension (from allowlist)', async () => {
      mockElectron.getAllowedExtensions.mockResolvedValue(['npx test-extension']);

      renderWithIntl(<ExtensionInstallModal addExtension={mockAddExtension} setView={mockSetView} />);

      const eventHandler = getAddExtensionEventHandler();

      await act(async () => {
        await eventHandler({}, 'goose://extension?cmd=npx&arg=test-extension&name=AllowedExt');
      });

      expect(screen.getByText('Confirm Extension Installation')).toBeInTheDocument();
      expect(screen.getAllByRole('button')).toHaveLength(3);
    });

    it('should handle warning mode', async () => {
      mockElectron.getConfig.mockReturnValue({
        GOOSE_ALLOWLIST_WARNING: true,
      });
      mockElectron.getAllowedExtensions.mockResolvedValue(['uvx allowed-package']);

      renderWithIntl(<ExtensionInstallModal addExtension={mockAddExtension} setView={mockSetView} />);

      const eventHandler = getAddExtensionEventHandler();

      await act(async () => {
        await eventHandler(
          {},
          'goose://extension?cmd=npx&arg=untrusted-extension&name=UntrustedExt'
        );
      });

      expect(screen.getByText('Install Untrusted Extension?')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Install Anyway' })).toBeInTheDocument();
      expect(screen.getAllByRole('button')).toHaveLength(3);
    });

    it('should handle i-ching-mcp-server as allowed command', async () => {
      mockElectron.getAllowedExtensions.mockResolvedValue([]);

      renderWithIntl(<ExtensionInstallModal addExtension={mockAddExtension} setView={mockSetView} />);

      const eventHandler = getAddExtensionEventHandler();

      await act(async () => {
        await eventHandler(
          {},
          'goose://extension?cmd=i-ching-mcp-server&id=i-ching&name=I%20Ching&description=I%20Ching%20divination'
        );
      });

      expect(screen.getByRole('dialog')).toBeInTheDocument();
      expect(screen.getByText('Confirm Extension Installation')).toBeInTheDocument();
      expect(screen.getByText(/I Ching extension/)).toBeInTheDocument();
      expect(screen.getAllByRole('button')).toHaveLength(3);
    });
    it('should handle blocked extension', async () => {
      mockElectron.getAllowedExtensions.mockResolvedValue(['uvx allowed-package']);

      renderWithIntl(<ExtensionInstallModal addExtension={mockAddExtension} setView={mockSetView} />);

      const eventHandler = getAddExtensionEventHandler();

      await act(async () => {
        await eventHandler({}, 'goose://extension?cmd=npx&arg=blocked-extension&name=BlockedExt');
      });

      expect(screen.getByText('Extension Installation Blocked')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'OK' })).toBeInTheDocument();
      expect(screen.getAllByRole('button')).toHaveLength(2);
      expect(screen.getByText(/Contact your administrator/)).toBeInTheDocument();
    });
  });

  describe('Modal Actions', () => {
    it('should dismiss modal correctly', async () => {
      mockElectron.getAllowedExtensions.mockResolvedValue([]);

      renderWithIntl(<ExtensionInstallModal addExtension={mockAddExtension} setView={mockSetView} />);

      const eventHandler = getAddExtensionEventHandler();

      await act(async () => {
        await eventHandler({}, 'goose://extension?cmd=npx&arg=test&name=Test');
      });

      expect(screen.getByRole('dialog')).toBeInTheDocument();

      await act(async () => {
        screen.getByRole('button', { name: 'No' }).click();
      });

      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });

    it('should handle successful extension installation', async () => {
      vi.mocked(addExtensionFromDeepLink).mockResolvedValue(undefined);
      mockElectron.getAllowedExtensions.mockResolvedValue([]);

      renderWithIntl(<ExtensionInstallModal addExtension={mockAddExtension} setView={mockSetView} />);

      const eventHandler = getAddExtensionEventHandler();

      await act(async () => {
        await eventHandler({}, 'goose://extension?cmd=npx&arg=test&name=Test');
      });

      await act(async () => {
        screen.getByRole('button', { name: 'Yes' }).click();
      });

      expect(addExtensionFromDeepLink).toHaveBeenCalledWith(
        'goose://extension?cmd=npx&arg=test&name=Test',
        mockAddExtension,
        expect.any(Function)
      );

      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
  });
});
