import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import { ClientExtensionChatActions } from './ClientExtensionChatActions';
import { ClientExtensionsProvider } from './ClientExtensionsContext';
import type { DiscoveredClientExtension } from './types';

const acpMocks = vi.hoisted(() => ({
  acpListClientExtensions: vi.fn(),
  acpReadClientExtensionMain: vi.fn(),
}));

vi.mock('../acp/clientExtensions', () => ({
  acpListClientExtensions: acpMocks.acpListClientExtensions,
  acpReadClientExtensionMain: acpMocks.acpReadClientExtensionMain,
  acpInstallClientExtension: vi.fn(),
  acpSetClientExtensionEnabled: vi.fn(),
  acpUninstallClientExtension: vi.fn(),
}));

vi.mock('../acp/providers', () => ({ acpListProviderDetails: vi.fn(), acpReadDefaults: vi.fn() }));

function chatActionExtension(): DiscoveredClientExtension {
  return {
    id: 'demo',
    source: 'installed',
    enabled: true,
    manifest: {
      id: 'demo',
      version: '1.0.0',
      runtime: 'sandbox',
      main: 'index.html',
      permissions: ['platform:read'],
      contributes: { chatActions: [{ id: 'recap', label: 'Recap' }] },
    },
  };
}

function renderChatActions(onSetInput = vi.fn()) {
  const utils = render(
    <MemoryRouter>
      <ClientExtensionsProvider>
        <ClientExtensionChatActions sessionId="s1" onSetInput={onSetInput} />
      </ClientExtensionsProvider>
    </MemoryRouter>,
    { wrapper: IntlTestWrapper }
  );
  return { ...utils, onSetInput };
}

beforeEach(() => {
  Reflect.set(window, 'electron', { platform: 'darwin', arch: 'arm64' });
});

afterEach(() => {
  Reflect.deleteProperty(window, 'electron');
  vi.clearAllMocks();
});

describe('ClientExtensionChatActions', () => {
  it('loads the runtime on click, activates it and delivers a host invoke', async () => {
    acpMocks.acpListClientExtensions.mockResolvedValue({
      installDir: '/plugins',
      extensions: [chatActionExtension()],
    });
    acpMocks.acpReadClientExtensionMain.mockResolvedValue('<html></html>');

    const { container } = renderChatActions();

    fireEvent.click(await screen.findByRole('button', { name: 'Recap' }));

    const iframe = await waitFor(() => {
      const found = container.querySelector<HTMLIFrameElement>(
        'iframe[title="demo runtime"]'
      );
      if (!found) {
        throw new Error('runtime iframe not mounted yet');
      }
      return found;
    });
    const postMessage = vi.fn();
    Object.defineProperty(iframe, 'contentWindow', { configurable: true, value: { postMessage } });

    await waitFor(() => {
      window.dispatchEvent(
        new MessageEvent('message', {
          data: { type: 'grc/host/invoke', capability: 'platform', method: 'getInfo', id: 'p1' },
          source: iframe.contentWindow as unknown as never,
        })
      );
      expect(postMessage).toHaveBeenCalledWith(
        {
          type: 'grc/host/result',
          capability: 'platform',
          method: 'getInfo',
          id: 'p1',
          payload: { platform: 'darwin', arch: 'arm64' },
        },
        '*'
      );
    });
  });

  it('routes setInput to the caller after the common host messages are handled', async () => {
    acpMocks.acpListClientExtensions.mockResolvedValue({
      installDir: '/plugins',
      extensions: [chatActionExtension()],
    });
    acpMocks.acpReadClientExtensionMain.mockResolvedValue('<html></html>');

    const { container, onSetInput } = renderChatActions();
    fireEvent.click(await screen.findByRole('button', { name: 'Recap' }));

    const iframe = await waitFor(() => {
      const found = container.querySelector<HTMLIFrameElement>('iframe[title="demo runtime"]');
      if (!found) {
        throw new Error('runtime iframe not mounted yet');
      }
      return found;
    });
    Object.defineProperty(iframe, 'contentWindow', {
      configurable: true,
      value: { postMessage: vi.fn() },
    });

    await waitFor(() => {
      window.dispatchEvent(
        new MessageEvent('message', {
          data: { type: 'grc/chat/setInput', text: 'hello from plugin' },
          source: iframe.contentWindow as unknown as never,
        })
      );
      expect(onSetInput).toHaveBeenCalledWith('hello from plugin');
    });
  });
});
