import { render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../../i18n/test-utils';
import McpAppRenderer from '../McpAppRenderer';
import type { GooseDisplayMode } from '../types';

vi.mock('@mcp-ui/client', () => ({
  AppBridge: class {
    onmessage = null;
    connect = vi.fn(() => new Promise(() => {}));
    close = vi.fn();
  },
  PostMessageTransport: class {},
}));

vi.mock('../../../acp/mcp-apps', () => ({
  readMcpAppResource: vi.fn(async () => ({ text: '<html></html>', _meta: {} })),
  callMcpAppTool: vi.fn(),
}));

vi.mock('../../../contexts/ThemeContext', () => ({
  useTheme: () => ({ resolvedTheme: 'light', mcpHostStyles: {} }),
}));

vi.mock('../../settings/extensions/subcomponents/ExtensionList', () => ({
  formatExtensionName: (name: string) => name,
}));

/**
 * Display modes must never remount the app iframe: a remount reloads the app
 * and loses its state. The shell around the iframe therefore keeps the same
 * element types in the same positions in every mode, and mode-specific chrome
 * renders as siblings, never as ancestors, of the app container.
 */
describe('McpAppRenderer display modes', () => {
  const electron = window.electron as unknown as Record<string, unknown>;

  beforeEach(() => {
    electron.getAcpUrl = vi.fn(async () => 'ws://127.0.0.1:3000/acp');
    electron.getSecretKey = vi.fn(async () => 'secret');
    window.matchMedia = vi
      .fn()
      .mockReturnValue({ matches: false }) as unknown as typeof window.matchMedia;
    window.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  });

  afterEach(() => {
    delete electron.getAcpUrl;
    delete electron.getSecretKey;
  });

  function renderApp(displayMode: GooseDisplayMode) {
    return (
      <McpAppRenderer
        resourceUri="ui://bench/app"
        extensionName="bench"
        sessionId="session-a"
        displayMode={displayMode}
      />
    );
  }

  it('keeps the same iframe attached through inline, pip and fullscreen', async () => {
    const { container, rerender } = render(renderApp('inline'), { wrapper: IntlTestWrapper });
    const iframe = await waitFor(() => {
      const el = container.querySelector('iframe');
      expect(el).not.toBeNull();
      return el as HTMLIFrameElement;
    });

    const removals: Node[] = [];
    const observer = new MutationObserver((records) => {
      for (const record of records) {
        removals.push(...Array.from(record.removedNodes));
      }
    });
    observer.observe(container, { childList: true, subtree: true });

    for (const mode of ['pip', 'fullscreen', 'inline', 'fullscreen', 'pip', 'inline'] as const) {
      rerender(renderApp(mode));
      expect(container.querySelector('iframe')).toBe(iframe);
      expect(iframe.isConnected).toBe(true);
    }

    observer.takeRecords().forEach((record) => removals.push(...Array.from(record.removedNodes)));
    observer.disconnect();
    const detachedIframe = removals.some(
      (node) => node === iframe || (node instanceof Element && node.contains(iframe))
    );
    expect(detachedIframe).toBe(false);
  });
});
