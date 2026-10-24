import { afterEach, describe, expect, it, vi } from 'vitest';
import { publishExtensionSessionEvent } from '../extensionSessionEvents';
import { createHostApi } from './hostApi';

const actions = {
  startChat: vi.fn(),
  createSession: vi.fn(),
  openSession: vi.fn(),
  openPage: vi.fn(),
};

const event = {
  type: 'status_message',
  sessionId: 's1',
  message: 'hi',
  level: 'notice',
} as const;

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe('createHostApi', () => {
  it('returns handler results for granted methods', async () => {
    vi.stubGlobal('window', { electron: { platform: 'darwin', arch: 'arm64' } });
    const api = createHostApi('demo', ['platform:read'], actions);

    await expect(api.invoke('platform', 'getInfo')).resolves.toEqual({
      platform: 'darwin',
      arch: 'arm64',
    });
  });

  it('rejects ungranted, unknown and prototype-key calls', async () => {
    const api = createHostApi('demo', [], actions);

    await expect(api.invoke('platform', 'getInfo')).rejects.toThrow(
      'Plugin "demo" is not granted permission "platform:read"'
    );
    await expect(api.invoke('nope', 'run')).rejects.toThrow('Unknown host capability "nope"');
    await expect(api.invoke('platform', 'toString')).rejects.toThrow(
      'Unknown method "platform.toString"'
    );
    await expect(api.invoke('constructor', 'run')).rejects.toThrow(
      'Unknown host capability "constructor"'
    );
  });

  it('exposes the granted permissions without duplicates', () => {
    const api = createHostApi('demo', ['sessions:read', 'sessions:read', 'recipes:read'], actions);

    expect(api.permissions).toEqual(['sessions:read', 'recipes:read']);
  });

  it('delivers capability events to subscribers until they unsubscribe', async () => {
    const api = createHostApi('demo', ['sessions:events'], actions);
    const listener = vi.fn();
    const unsubscribe = api.subscribe(listener);

    await api.invoke('sessions', 'subscribe');
    publishExtensionSessionEvent(event);
    expect(listener).toHaveBeenCalledWith('sessions', 'session', event);

    unsubscribe();
    listener.mockClear();
    publishExtensionSessionEvent(event);
    expect(listener).not.toHaveBeenCalled();
    api.dispose();
  });

  it('drops capability subscriptions on reset and everything on dispose', async () => {
    const api = createHostApi('demo', ['sessions:events'], actions);
    const listener = vi.fn();
    api.subscribe(listener);

    await api.invoke('sessions', 'subscribe');
    api.reset();
    publishExtensionSessionEvent(event);
    expect(listener).not.toHaveBeenCalled();

    await api.invoke('sessions', 'subscribe');
    api.dispose();
    publishExtensionSessionEvent(event);
    expect(listener).not.toHaveBeenCalled();
  });
});
