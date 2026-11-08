import { render, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import { ClientExtensionMessageDecorations } from './ClientExtensionMessageDecorations';
import { ClientExtensionsProvider } from './ClientExtensionsContext';
import type { Message } from '../types/message';
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

function decorationExtension(): DiscoveredClientExtension {
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
      contributes: { contentSuffixes: [{ id: 'badge' }] },
    },
  };
}

const message: Message = {
  content: [{ type: 'text', text: 'hi' }],
  created: 0,
  id: 'm1',
  metadata: { agentVisible: true, userVisible: true },
  role: 'assistant',
};

function renderDecorations() {
  return render(
    <MemoryRouter>
      <ClientExtensionsProvider>
        <ClientExtensionMessageDecorations
          sessionId="s1"
          message={message}
          displayText="hi"
          imageCount={0}
        />
      </ClientExtensionsProvider>
    </MemoryRouter>,
    { wrapper: IntlTestWrapper }
  );
}

beforeEach(() => {
  Reflect.set(window, 'electron', { platform: 'darwin', arch: 'arm64' });
});

afterEach(() => {
  Reflect.deleteProperty(window, 'electron');
  vi.clearAllMocks();
});

describe('ClientExtensionMessageDecorations', () => {
  it('routes a host invoke and still resizes on grc/resize', async () => {
    acpMocks.acpListClientExtensions.mockResolvedValue({
      installDir: '/plugins',
      extensions: [decorationExtension()],
    });
    acpMocks.acpReadClientExtensionMain.mockResolvedValue('<html></html>');

    const { container } = renderDecorations();

    const iframe = await waitFor(() => {
      const found = container.querySelector<HTMLIFrameElement>('iframe[title="demo:badge"]');
      if (!found) {
        throw new Error('render slot iframe not mounted yet');
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

    await waitFor(() => {
      window.dispatchEvent(
        new MessageEvent('message', {
          data: { type: 'grc/resize', height: 120 },
          source: iframe.contentWindow as unknown as never,
        })
      );
      expect(iframe.style.height).toBe('120px');
    });
  });
});
