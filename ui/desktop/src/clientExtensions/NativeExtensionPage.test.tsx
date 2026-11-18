import { render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import ClientExtensionPageView from './ClientExtensionPageView';
import { ClientExtensionsProvider } from './ClientExtensionsContext';
import { NativePluginRuntime } from './NativePluginRuntime';
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

vi.mock('../sessions', () => ({ startNewSession: vi.fn() }));

vi.mock('../hooks/useNavigationSessions', () => ({
  useNavigationSessions: () => ({ handleNavClick: vi.fn() }),
}));

vi.mock('./nativePlugin', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./nativePlugin')>();
  const { evalPluginCode } = await import('./nativePluginTestHelpers');
  return { ...actual, loadPluginDefinition: evalPluginCode };
});

function nativeExtension(overrides: Partial<DiscoveredClientExtension> = {}) {
  return {
    id: 'demo',
    source: 'installed',
    enabled: true,
    manifest: {
      id: 'demo',
      version: '1.0.0',
      runtime: 'native',
      main: 'index.js',
      contributes: { rootLinks: [{ id: 'home', label: 'Demo dashboard' }] },
    },
    ...overrides,
  } satisfies DiscoveredClientExtension;
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={['/ext/demo/home']}>
      <ClientExtensionsProvider>
        <NativePluginRuntime />
        <Routes>
          <Route path="/ext/:extensionId/:viewId" element={<ClientExtensionPageView />} />
        </Routes>
      </ClientExtensionsProvider>
    </MemoryRouter>,
    { wrapper: IntlTestWrapper }
  );
}

beforeEach(() => {
  Reflect.set(window, 'electron', { getVersion: () => '1.52.0' });
});

afterEach(() => {
  Reflect.deleteProperty(window, 'electron');
  vi.clearAllMocks();
});

describe('native plugin page', () => {
  it('loads the plugin code and renders the page it registers', async () => {
    acpMocks.acpListClientExtensions.mockResolvedValue({
      installDir: '/plugins',
      extensions: [nativeExtension()],
    });
    acpMocks.acpReadClientExtensionMain.mockResolvedValue(`defineGoosePlugin({
      activate(api) {
        const h = api.react.createElement;
        api.pages.register('home', function Home() {
          return h('p', null, 'Hello from ' + api.extensionId);
        });
      }
    });`);

    renderPage();

    expect(await screen.findByText('Hello from demo')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Demo dashboard' })).toBeInTheDocument();
  });

  it('contains a plugin that crashes while rendering', async () => {
    acpMocks.acpListClientExtensions.mockResolvedValue({
      installDir: '/plugins',
      extensions: [nativeExtension()],
    });
    acpMocks.acpReadClientExtensionMain.mockResolvedValue(`defineGoosePlugin({
      activate(api) {
        api.pages.register('home', function Home() {
          throw new Error('plugin bug');
        });
      }
    });`);

    renderPage();

    expect(await screen.findByText('Failed to load plugin "demo"')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Demo dashboard' })).toBeInTheDocument();
  });

  it('reports a plugin whose activation throws', async () => {
    acpMocks.acpListClientExtensions.mockResolvedValue({
      installDir: '/plugins',
      extensions: [nativeExtension()],
    });
    acpMocks.acpReadClientExtensionMain.mockResolvedValue(`defineGoosePlugin({
      activate() { throw new Error('activation bug'); }
    });`);

    renderPage();

    expect(await screen.findByText('Failed to load plugin "demo"')).toBeInTheDocument();
  });

  it('does not run disabled plugins', async () => {
    acpMocks.acpListClientExtensions.mockResolvedValue({
      installDir: '/plugins',
      extensions: [nativeExtension({ enabled: false })],
    });

    renderPage();

    await vi.waitFor(() => expect(acpMocks.acpListClientExtensions).toHaveBeenCalled());
    expect(acpMocks.acpReadClientExtensionMain).not.toHaveBeenCalled();
  });
});
