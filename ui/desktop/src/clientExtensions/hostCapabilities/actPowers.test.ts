import { afterEach, describe, expect, it, vi } from 'vitest';
import { createHostSession } from './session';
import { registerPluginCommand, unregisterPluginCommands } from '../pluginCommandRegistry';
import type { HostPermission } from './permissions';

const acpMocks = vi.hoisted(() => ({
  listRecipes: vi.fn(),
  acpGetClientExtensionStorage: vi.fn(),
  acpSetClientExtensionStorage: vi.fn(),
  acpDeleteClientExtensionStorage: vi.fn(),
  acpListClientExtensionStorageKeys: vi.fn(),
}));

vi.mock('../../acp/recipe', () => ({
  listRecipes: acpMocks.listRecipes,
}));

vi.mock('../../acp/clientExtensions', () => ({
  acpGetClientExtensionStorage: acpMocks.acpGetClientExtensionStorage,
  acpSetClientExtensionStorage: acpMocks.acpSetClientExtensionStorage,
  acpDeleteClientExtensionStorage: acpMocks.acpDeleteClientExtensionStorage,
  acpListClientExtensionStorageKeys: acpMocks.acpListClientExtensionStorageKeys,
}));

const actions = {
  startChat: vi.fn(),
  createSession: vi.fn(),
  openSession: vi.fn(),
  openPage: vi.fn(),
};

async function invoke(
  extensionId: string,
  permissions: HostPermission[],
  capability: string,
  method: string,
  payload?: unknown
) {
  const post = vi.fn();
  await createHostSession(extensionId, permissions, post, actions).handleInvoke({
    type: 'grc/host/invoke',
    capability,
    method,
    payload,
  });
  return post.mock.calls[0][0];
}

afterEach(() => {
  unregisterPluginCommands('a-plugin');
  unregisterPluginCommands('b-plugin');
  vi.clearAllMocks();
});

describe('commands power', () => {
  it('lists the core commands', async () => {
    const result = await invoke('demo', ['commands:execute'], 'commands', 'list');

    expect(result.payload.map((command: { id: string }) => command.id)).toEqual([
      'chat.new',
      'session.open',
      'plugin.open',
    ]);
  });

  it('starts a chat with a prompt and a recipe', async () => {
    actions.startChat.mockResolvedValue('session-1');

    const result = await invoke('demo', ['commands:execute'], 'commands', 'execute', {
      command: 'chat.new',
      args: { prompt: 'review my diff', recipeId: 'review' },
    });

    expect(actions.startChat).toHaveBeenCalledWith({
      prompt: 'review my diff',
      recipeId: 'review',
    });
    expect(result.payload).toEqual({ sessionId: 'session-1' });
  });

  it('opens sessions and plugin pages', async () => {
    await invoke('demo', ['commands:execute'], 'commands', 'execute', {
      command: 'session.open',
      args: { sessionId: 's9' },
    });
    await invoke('demo', ['commands:execute'], 'commands', 'execute', {
      command: 'plugin.open',
      args: { extensionId: 'other', viewId: 'home' },
    });

    expect(actions.openSession).toHaveBeenCalledWith('s9');
    expect(actions.openPage).toHaveBeenCalledWith('other', 'home');
  });

  it('rejects unknown commands, prototype keys and invalid arguments', async () => {
    const unknown = await invoke('demo', ['commands:execute'], 'commands', 'execute', {
      command: 'nope',
    });
    const prototypeKey = await invoke('demo', ['commands:execute'], 'commands', 'execute', {
      command: 'constructor',
    });
    const invalid = await invoke('demo', ['commands:execute'], 'commands', 'execute', {
      command: 'session.open',
      args: {},
    });
    const oversized = await invoke('demo', ['commands:execute'], 'commands', 'execute', {
      command: 'chat.new',
      args: { prompt: 'x'.repeat(20_001) },
    });

    expect(unknown.error).toBe('Unknown command "nope"');
    expect(prototypeKey.error).toBe('Unknown command "constructor"');
    expect(invalid.error).toContain('Invalid "sessionId"');
    expect(oversized.error).toContain('Invalid "prompt"');
    expect(actions.startChat).not.toHaveBeenCalled();
  });

  it('lists and runs plugin-registered commands alongside the core ones', async () => {
    const run = vi.fn().mockReturnValue({ done: true });
    registerPluginCommand('a-plugin', 'refresh', 'Refresh the view', run);

    const list = await invoke('demo', ['commands:execute'], 'commands', 'list');
    expect(list.payload).toContainEqual({
      id: 'a-plugin:refresh',
      description: 'Refresh the view',
    });

    const result = await invoke('demo', ['commands:execute'], 'commands', 'execute', {
      command: 'a-plugin:refresh',
      args: { x: 1 },
    });
    expect(run).toHaveBeenCalledWith({ x: 1 });
    expect(result.payload).toEqual({ done: true });
  });

  it('keeps two plugins in separate namespaces and drops them independently', async () => {
    registerPluginCommand('a-plugin', 'go', 'Go', vi.fn());
    registerPluginCommand('b-plugin', 'go', 'Go', vi.fn());

    const beforeIds = (await invoke('demo', ['commands:execute'], 'commands', 'list')).payload.map(
      (c: { id: string }) => c.id
    );
    expect(beforeIds).toEqual(expect.arrayContaining(['a-plugin:go', 'b-plugin:go']));

    unregisterPluginCommands('a-plugin');
    const afterIds = (await invoke('demo', ['commands:execute'], 'commands', 'list')).payload.map(
      (c: { id: string }) => c.id
    );
    expect(afterIds).not.toContain('a-plugin:go');
    expect(afterIds).toContain('b-plugin:go');
  });

  it('requires commands:execute', async () => {
    const result = await invoke('demo', ['sessions:read'], 'commands', 'execute', {
      command: 'chat.new',
    });

    expect(result.error).toContain('commands:execute');
    expect(actions.startChat).not.toHaveBeenCalled();
  });
});

describe('recipes power', () => {
  it('projects the recipe library without instructions or file paths', async () => {
    acpMocks.listRecipes.mockResolvedValue([
      {
        id: 'review',
        recipe: { title: 'Review', description: 'Review a diff', instructions: 'secret' },
        file_path: '/home/me/review.yaml',
        last_modified: '2026-09-01T00:00:00Z',
        schedule_cron: '0 9 * * *',
      },
    ]);

    const result = await invoke('demo', ['recipes:read'], 'recipes', 'list');

    expect(result.payload).toEqual([
      {
        id: 'review',
        title: 'Review',
        description: 'Review a diff',
        lastModified: '2026-09-01T00:00:00Z',
        scheduleCron: '0 9 * * *',
        slashCommand: null,
      },
    ]);
  });

  it('requires recipes:read', async () => {
    const result = await invoke('demo', [], 'recipes', 'list');

    expect(result.error).toContain('recipes:read');
  });
});

describe('storage power', () => {
  const grant: HostPermission[] = ['storage:readwrite'];

  it('reads, writes, deletes and lists through the backend, scoped to the caller', async () => {
    acpMocks.acpGetClientExtensionStorage.mockResolvedValue({ theme: 'dark' });
    acpMocks.acpDeleteClientExtensionStorage.mockResolvedValue(true);
    acpMocks.acpListClientExtensionStorageKeys.mockResolvedValue(['prefs', 'other']);

    const set = await invoke('demo', grant, 'storage', 'set', {
      key: 'prefs',
      value: { theme: 'dark' },
    });
    const get = await invoke('demo', grant, 'storage', 'get', { key: 'prefs' });
    const keys = await invoke('demo', grant, 'storage', 'keys');
    const del = await invoke('demo', grant, 'storage', 'delete', { key: 'other' });

    expect(acpMocks.acpSetClientExtensionStorage).toHaveBeenCalledWith('demo', 'prefs', {
      theme: 'dark',
    });
    expect(acpMocks.acpGetClientExtensionStorage).toHaveBeenCalledWith('demo', 'prefs');
    expect(acpMocks.acpListClientExtensionStorageKeys).toHaveBeenCalledWith('demo');
    expect(acpMocks.acpDeleteClientExtensionStorage).toHaveBeenCalledWith('demo', 'other');

    expect(set.payload).toEqual({ key: 'prefs' });
    expect(get.payload).toEqual({ theme: 'dark' });
    expect(keys.payload).toEqual(['prefs', 'other']);
    expect(del.payload).toEqual({ key: 'other', existed: true });
  });

  it('uses the host session identity, not a key a plugin could supply itself', async () => {
    await invoke('plugin-a', grant, 'storage', 'get', { key: 'k' });
    await invoke('plugin-b', grant, 'storage', 'get', { key: 'k' });

    expect(acpMocks.acpGetClientExtensionStorage).toHaveBeenNthCalledWith(1, 'plugin-a', 'k');
    expect(acpMocks.acpGetClientExtensionStorage).toHaveBeenNthCalledWith(2, 'plugin-b', 'k');
  });

  it('surfaces the backend error when a limit is exceeded', async () => {
    acpMocks.acpSetClientExtensionStorage.mockRejectedValue(
      new Error('plugin storage is limited to 200 keys')
    );

    const result = await invoke('demo', grant, 'storage', 'set', { key: 'extra', value: 1 });

    expect(result.error).toContain('200 keys');
  });

  it('requires storage:readwrite and a well-formed key', async () => {
    const denied = await invoke('demo', [], 'storage', 'get', { key: 'k' });
    const missing = await invoke('demo', grant, 'storage', 'get', {});

    expect(denied.error).toContain('storage:readwrite');
    expect(missing.error).toContain('Invalid "key"');
    expect(acpMocks.acpGetClientExtensionStorage).not.toHaveBeenCalled();
  });
});
