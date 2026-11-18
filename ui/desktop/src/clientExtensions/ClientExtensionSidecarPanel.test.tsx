import { render, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import {
  ClientExtensionSidecarControls,
  ClientExtensionSidecarPanel,
  ClientExtensionSidecarProvider,
} from './ClientExtensionSidecarPanel';
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

function sidecarExtension(): DiscoveredClientExtension {
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
      contributes: { sidecars: [{ id: 'panel', label: 'Demo panel', defaultOpen: true }] },
    },
  };
}

function renderSidecar() {
  return render(
    <MemoryRouter>
      <ClientExtensionsProvider>
        <ClientExtensionSidecarProvider sessionId="s1">
          <ClientExtensionSidecarControls />
          <ClientExtensionSidecarPanel />
        </ClientExtensionSidecarProvider>
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

describe('ClientExtensionSidecarPanel', () => {
  it('routes a host invoke from the sidecar iframe and replies with the result', async () => {
    acpMocks.acpListClientExtensions.mockResolvedValue({
      installDir: '/plugins',
      extensions: [sidecarExtension()],
    });
    acpMocks.acpReadClientExtensionMain.mockResolvedValue('<html></html>');

    const { container } = renderSidecar();

    const iframe = await waitFor(() => {
      const found = container.querySelector<HTMLIFrameElement>('iframe[title="Demo panel"]');
      if (!found) {
        throw new Error('iframe not rendered yet');
      }
      return found;
    });
    const postMessage = vi.fn();
    Object.defineProperty(iframe, 'contentWindow', {
      configurable: true,
      value: { postMessage },
    });

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
});
