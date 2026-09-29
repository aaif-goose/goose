import { afterEach, describe, expect, it, vi } from 'vitest';
import { publishExtensionSessionEvent } from '../extensionSessionEvents';
import { HOST_PERMISSIONS } from './permissions';
import { COMMON_HOST_POWERS } from './powers';
import { createHostSession } from './session';

const acpMocks = vi.hoisted(() => ({
  acpListProviderDetails: vi.fn(),
  acpReadDefaults: vi.fn(),
  acpSaveDefaults: vi.fn(),
  acpListRecentSessions: vi.fn(),
}));

vi.mock('../../acp/providers', () => ({
  acpListProviderDetails: acpMocks.acpListProviderDetails,
  acpReadDefaults: acpMocks.acpReadDefaults,
  acpSaveDefaults: acpMocks.acpSaveDefaults,
}));

vi.mock('../../acp/sessions', () => ({
  acpListRecentSessions: acpMocks.acpListRecentSessions,
}));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe('hostCapabilities', () => {
  it('keeps every method permission declared and every permission in use', () => {
    const used = new Set<string>();
    for (const power of COMMON_HOST_POWERS) {
      for (const method of Object.values(power.methods)) {
        expect(HOST_PERMISSIONS).toContain(method.permission);
        used.add(method.permission);
      }
    }
    expect([...used].sort()).toEqual([...HOST_PERMISSIONS].sort());
  });

  it('rejects invoke when the permission is not granted', async () => {
    const post = vi.fn();
    const session = createHostSession('demo', [], post);

    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'platform',
      method: 'getInfo',
      id: 'a',
    });

    expect(post).toHaveBeenCalledWith({
      type: 'grc/host/error',
      capability: 'platform',
      method: 'getInfo',
      id: 'a',
      error: expect.stringContaining('not granted permission "platform:read"'),
    });
  });

  it('rejects unknown capabilities and methods, including prototype keys', async () => {
    const post = vi.fn();
    const session = createHostSession('demo', ['platform:read'], post);

    await session.handleInvoke({ type: 'grc/host/invoke', capability: 'nope', method: 'run' });
    await session.handleInvoke({ type: 'grc/host/invoke', capability: 'platform', method: 'nope' });
    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'constructor',
      method: 'run',
    });
    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'platform',
      method: 'toString',
    });

    expect(post.mock.calls.map(([message]) => message.error)).toEqual([
      'Unknown host capability "nope"',
      'Unknown method "platform.nope"',
      'Unknown host capability "constructor"',
      'Unknown method "platform.toString"',
    ]);
  });

  it('returns platform info and echoes the call id', async () => {
    vi.stubGlobal('window', { electron: { platform: 'darwin', arch: 'arm64' } });
    const post = vi.fn();
    const session = createHostSession('demo', ['platform:read'], post);

    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'platform',
      method: 'getInfo',
      id: 'p1',
    });

    expect(post).toHaveBeenCalledWith({
      type: 'grc/host/result',
      capability: 'platform',
      method: 'getInfo',
      id: 'p1',
      payload: { platform: 'darwin', arch: 'arm64' },
    });
  });

  it('announces granted permissions', () => {
    const post = vi.fn();
    createHostSession('demo', ['sessions:read', 'providers:read'], post).notifyPermissions();

    expect(post).toHaveBeenCalledWith({
      type: 'grc/host/permissions',
      permissions: ['sessions:read', 'providers:read'],
    });
  });

  it('projects provider inventory to non-sensitive fields', async () => {
    acpMocks.acpListProviderDetails.mockResolvedValue([
      {
        name: 'openai',
        is_configured: true,
        is_available: true,
        metadata: { display_name: 'OpenAI', default_model: 'gpt', config_keys: [{ name: 'KEY' }] },
        saved_model: 'gpt',
      },
    ]);
    const post = vi.fn();
    const session = createHostSession('demo', ['providers:read'], post);

    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'providers',
      method: 'list',
    });

    expect(post).toHaveBeenCalledWith({
      type: 'grc/host/result',
      capability: 'providers',
      method: 'list',
      id: undefined,
      payload: [
        {
          id: 'openai',
          displayName: 'OpenAI',
          configured: true,
          available: true,
          defaultModel: 'gpt',
        },
      ],
    });
  });

  it('requires providers:write and a providerId to change the default', async () => {
    const readOnlyPost = vi.fn();
    await createHostSession('demo', ['providers:read'], readOnlyPost).handleInvoke({
      type: 'grc/host/invoke',
      capability: 'providers',
      method: 'setDefault',
      payload: { providerId: 'openai' },
    });
    expect(readOnlyPost.mock.calls[0][0].error).toContain('providers:write');
    expect(acpMocks.acpSaveDefaults).not.toHaveBeenCalled();

    const post = vi.fn();
    const session = createHostSession('demo', ['providers:write'], post);
    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'providers',
      method: 'setDefault',
      payload: {},
    });
    expect(post.mock.calls[0][0].error).toContain('Invalid "providerId"');

    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'providers',
      method: 'setDefault',
      payload: { providerId: 'openai', modelId: 'gpt' },
    });
    expect(acpMocks.acpSaveDefaults).toHaveBeenCalledWith('openai', 'gpt');
  });

  it('clamps the session list limit', async () => {
    acpMocks.acpListRecentSessions.mockResolvedValue([]);
    const session = createHostSession('demo', ['sessions:read'], vi.fn());

    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'sessions',
      method: 'list',
      payload: { limit: 10_000 },
    });
    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'sessions',
      method: 'list',
      payload: { limit: 'many' },
    });

    expect(acpMocks.acpListRecentSessions.mock.calls.map(([limit]) => limit)).toEqual([100, 50]);
  });

  it('streams session events until unsubscribe or dispose', async () => {
    const post = vi.fn();
    const session = createHostSession('demo', ['sessions:events'], post);
    const event = {
      type: 'status_message',
      sessionId: 's1',
      message: 'hi',
      level: 'notice',
    } as const;

    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'sessions',
      method: 'subscribe',
    });
    publishExtensionSessionEvent(event);
    expect(post).toHaveBeenCalledWith({
      type: 'grc/host/event',
      capability: 'sessions',
      event: 'session',
      payload: event,
    });

    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'sessions',
      method: 'unsubscribe',
    });
    post.mockClear();
    publishExtensionSessionEvent(event);
    expect(post).not.toHaveBeenCalled();

    await session.handleInvoke({
      type: 'grc/host/invoke',
      capability: 'sessions',
      method: 'subscribe',
    });
    post.mockClear();
    session.dispose();
    publishExtensionSessionEvent(event);
    expect(post).not.toHaveBeenCalled();
  });

  it('does not stack subscriptions when subscribe is called twice', async () => {
    const post = vi.fn();
    const session = createHostSession('demo', ['sessions:events'], post);
    const event = {
      type: 'status_message',
      sessionId: 's1',
      message: 'hi',
      level: 'notice',
    } as const;

    for (let i = 0; i < 2; i++) {
      await session.handleInvoke({
        type: 'grc/host/invoke',
        capability: 'sessions',
        method: 'subscribe',
      });
    }
    post.mockClear();
    publishExtensionSessionEvent(event);

    expect(post).toHaveBeenCalledTimes(1);
    session.dispose();
  });
});
