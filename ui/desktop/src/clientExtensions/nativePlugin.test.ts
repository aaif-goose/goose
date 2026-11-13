import { afterEach, describe, expect, it, vi } from 'vitest';
import { publishExtensionSessionEvent } from './extensionSessionEvents';
import { evalPluginCode } from './nativePluginTestHelpers';
import {
  activateNativePlugin,
  deactivateNativePlugin,
  getNativePluginPage,
  hasNativePluginFailed,
} from './nativePluginStore';
import type { DiscoveredClientExtension } from './types';

vi.mock('./nativePlugin', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./nativePlugin')>();
  const { evalPluginCode: evaluate } = await import('./nativePluginTestHelpers');
  return { ...actual, loadPluginDefinition: evaluate };
});

const actions = {
  startChat: vi.fn(),
  createSession: vi.fn(),
  openSession: vi.fn(),
  openPage: vi.fn(),
};

function extension(
  permissions: DiscoveredClientExtension['manifest']['permissions'] = []
): DiscoveredClientExtension {
  return {
    id: 'demo',
    source: 'installed',
    enabled: true,
    manifest: { id: 'demo', version: '1.0.0', runtime: 'native', main: 'index.js', permissions },
  };
}

afterEach(() => {
  deactivateNativePlugin('demo');
  Reflect.deleteProperty(window, 'electron');
  Reflect.deleteProperty(window, '__result');
  vi.clearAllMocks();
});

describe('evalPluginCode (the wrapping contract loadPluginDefinition also uses)', () => {
  it('runs the code and returns the definition it registered', () => {
    const definition = evalPluginCode('defineGoosePlugin({ activate() {}, deactivate() {} });');

    expect(typeof definition.activate).toBe('function');
    expect(typeof definition.deactivate).toBe('function');
  });

  it('rejects code that never registers a plugin', () => {
    expect(() => evalPluginCode('1 + 1;')).toThrow('defineGoosePlugin');
    expect(() => evalPluginCode('defineGoosePlugin({ activate: 1 });')).toThrow(
      'defineGoosePlugin'
    );
  });

  it('propagates a synchronous throw in the plugin code', () => {
    expect(() => evalPluginCode('throw new Error("bad plugin");')).toThrow('bad plugin');
  });

  it('keeps two plugins loaded back to back from colliding on shared top-level names', () => {
    const first = evalPluginCode(
      'const shared = "a"; defineGoosePlugin({ activate() { window.__a = shared; } });'
    );
    const second = evalPluginCode(
      'const shared = "b"; defineGoosePlugin({ activate() { window.__b = shared; } });'
    );

    first.activate({} as never);
    second.activate({} as never);
    expect(Reflect.get(window, '__a')).toBe('a');
    expect(Reflect.get(window, '__b')).toBe('b');
    Reflect.deleteProperty(window, '__a');
    Reflect.deleteProperty(window, '__b');
  });
});

describe('activateNativePlugin', () => {
  it('lets the plugin register pages that the store then serves', async () => {
    await activateNativePlugin(
      extension(),
      `defineGoosePlugin({
        activate(api) {
          api.pages.register('home', function Home() { return null; });
        }
      });`,
      actions
    );

    expect(getNativePluginPage('demo', 'home')).toBeTypeOf('function');
    expect(getNativePluginPage('demo', 'other')).toBeUndefined();
    expect(getNativePluginPage('missing', 'home')).toBeUndefined();
  });

  it('enforces the manifest permissions on host calls', async () => {
    Reflect.set(window, 'electron', { platform: 'darwin', arch: 'arm64' });
    const code = `defineGoosePlugin({
      async activate(api) {
        try {
          window.__result = await api.host.platform.getInfo();
        } catch (error) {
          window.__result = error.message;
        }
      }
    });`;

    await activateNativePlugin(extension([]), code, actions);
    expect(Reflect.get(window, '__result')).toBe(
      'Plugin "demo" is not granted permission "platform:read"'
    );

    await activateNativePlugin(extension(['platform:read']), code, actions);
    expect(Reflect.get(window, '__result')).toEqual({ platform: 'darwin', arch: 'arm64' });
  });

  it('exposes every host capability as callable methods', async () => {
    await activateNativePlugin(
      extension(),
      `defineGoosePlugin({
        activate(api) {
          window.__result = Object.fromEntries(
            Object.entries(api.host).map(([name, methods]) => [name, Object.keys(methods)])
          );
        }
      });`,
      actions
    );

    const result = Reflect.get(window, '__result') as Record<string, string[]>;
    expect(Object.keys(result).sort()).toEqual([
      'commands',
      'net',
      'platform',
      'providers',
      'recipes',
      'schedules',
      'sessions',
      'storage',
      'tools',
    ]);
    expect(result.commands).toEqual(['list', 'execute']);
  });

  it('lets a plugin register a command another plugin can list and run, until it deactivates', async () => {
    const seen: unknown[] = [];
    Reflect.set(window, '__seen', seen);

    await activateNativePlugin(
      extension(['commands:execute']),
      `defineGoosePlugin({
        activate(api) {
          api.commands.register('refresh', 'Refresh the view', (args) => {
            window.__seen.push(args);
            return { ok: true };
          });
        }
      });`,
      actions
    );

    const other = { ...extension(['commands:execute']), id: 'other' };
    await activateNativePlugin(
      other,
      `defineGoosePlugin({
        async activate(api) {
          window.__listIds = (await api.host.commands.list()).map((c) => c.id);
          window.__result = await api.host.commands.execute({
            command: 'demo:refresh',
            args: { x: 1 },
          });
        }
      });`,
      actions
    );

    expect(Reflect.get(window, '__listIds')).toContain('demo:refresh');
    expect(Reflect.get(window, '__result')).toEqual({ ok: true });
    expect(seen).toEqual([{ x: 1 }]);

    deactivateNativePlugin('demo');
    await activateNativePlugin(
      other,
      `defineGoosePlugin({
        async activate(api) {
          window.__listIds = (await api.host.commands.list()).map((c) => c.id);
        }
      });`,
      actions
    );

    expect(Reflect.get(window, '__listIds')).not.toContain('demo:refresh');
    Reflect.deleteProperty(window, '__seen');
    Reflect.deleteProperty(window, '__listIds');
    deactivateNativePlugin('other');
  });

  it('delivers capability events and stops after deactivation', async () => {
    const received = vi.fn();
    Reflect.set(window, '__received', received);
    await activateNativePlugin(
      extension(['sessions:events']),
      `defineGoosePlugin({
        async activate(api) {
          api.on('sessions', 'session', window.__received);
          await api.host.sessions.subscribe();
        }
      });`,
      actions
    );
    const event = {
      type: 'status_message',
      sessionId: 's1',
      message: 'hi',
      level: 'notice',
    } as const;

    publishExtensionSessionEvent(event);
    expect(received).toHaveBeenCalledWith(event);

    deactivateNativePlugin('demo');
    received.mockClear();
    publishExtensionSessionEvent(event);
    expect(received).not.toHaveBeenCalled();
    Reflect.deleteProperty(window, '__received');
  });

  it('calls deactivate and drops the plugin pages', async () => {
    const deactivated = vi.fn();
    Reflect.set(window, '__deactivated', deactivated);
    await activateNativePlugin(
      extension(),
      `defineGoosePlugin({
        activate(api) { api.pages.register('home', () => null); },
        deactivate() { window.__deactivated(); }
      });`,
      actions
    );

    deactivateNativePlugin('demo');

    expect(deactivated).toHaveBeenCalledTimes(1);
    expect(getNativePluginPage('demo', 'home')).toBeUndefined();
    Reflect.deleteProperty(window, '__deactivated');
  });

  it('replaces a previous activation of the same plugin', async () => {
    const first = `defineGoosePlugin({ activate(api) { api.pages.register('old', () => null); } });`;
    const second = `defineGoosePlugin({ activate(api) { api.pages.register('new', () => null); } });`;

    await activateNativePlugin(extension(), first, actions);
    await activateNativePlugin(extension(), second, actions);

    expect(getNativePluginPage('demo', 'old')).toBeUndefined();
    expect(getNativePluginPage('demo', 'new')).toBeTypeOf('function');
  });

  it('discards a superseded activation instead of registering its pages', async () => {
    const slow = `defineGoosePlugin({
      async activate(api) {
        await new Promise((resolve) => setTimeout(resolve, 20));
        api.pages.register('slow', () => null);
      }
    });`;
    const fast = `defineGoosePlugin({ activate(api) { api.pages.register('fast', () => null); } });`;

    const slowActivation = activateNativePlugin(extension(), slow, actions);
    await activateNativePlugin(extension(), fast, actions);
    await slowActivation;

    expect(getNativePluginPage('demo', 'fast')).toBeTypeOf('function');
    expect(getNativePluginPage('demo', 'slow')).toBeUndefined();
  });

  it('marks the plugin failed and removes it when activation throws', async () => {
    await expect(
      activateNativePlugin(
        extension(),
        `defineGoosePlugin({
          activate(api) {
            api.pages.register('home', () => null);
            throw new Error('boom');
          }
        });`,
        actions
      )
    ).rejects.toThrow('boom');

    expect(hasNativePluginFailed('demo')).toBe(true);
    expect(getNativePluginPage('demo', 'home')).toBeUndefined();

    deactivateNativePlugin('demo');
    expect(hasNativePluginFailed('demo')).toBe(false);
  });

  it('marks the plugin failed when the code is not a plugin', async () => {
    await expect(activateNativePlugin(extension(), 'const a = 1;', actions)).rejects.toThrow(
      'defineGoosePlugin'
    );

    expect(hasNativePluginFailed('demo')).toBe(true);
  });
});
