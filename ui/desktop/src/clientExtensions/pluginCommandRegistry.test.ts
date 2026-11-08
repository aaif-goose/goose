import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  findPluginCommand,
  listPluginCommands,
  pluginCommandId,
  registerPluginCommand,
  unregisterPluginCommands,
} from './pluginCommandRegistry';

afterEach(() => {
  unregisterPluginCommands('demo');
  unregisterPluginCommands('other');
});

describe('pluginCommandRegistry', () => {
  it('namespaces a command id by the registering extension', () => {
    expect(pluginCommandId('demo', 'refresh')).toBe('demo:refresh');
  });

  it('finds and lists a registered command under its namespaced id', () => {
    const run = vi.fn();
    registerPluginCommand('demo', 'refresh', 'Refresh', run);

    expect(listPluginCommands()).toEqual([{ id: 'demo:refresh', description: 'Refresh' }]);
    expect(findPluginCommand('demo:refresh')).toEqual({
      id: 'demo:refresh',
      description: 'Refresh',
      run,
    });
    expect(findPluginCommand('refresh')).toBeUndefined();
  });

  it('replaces a command re-registered under the same id', () => {
    registerPluginCommand('demo', 'refresh', 'First', vi.fn());
    registerPluginCommand('demo', 'refresh', 'Second', vi.fn());

    expect(listPluginCommands()).toEqual([{ id: 'demo:refresh', description: 'Second' }]);
  });

  it('keeps commands from different extensions independent', () => {
    registerPluginCommand('demo', 'go', 'Demo go', vi.fn());
    registerPluginCommand('other', 'go', 'Other go', vi.fn());

    expect(
      listPluginCommands()
        .map((c) => c.id)
        .sort()
    ).toEqual(['demo:go', 'other:go']);

    unregisterPluginCommands('demo');

    expect(listPluginCommands()).toEqual([{ id: 'other:go', description: 'Other go' }]);
  });

  it('drops all of one extension without touching another', () => {
    registerPluginCommand('demo', 'a', 'A', vi.fn());
    registerPluginCommand('demo', 'b', 'B', vi.fn());
    registerPluginCommand('other', 'c', 'C', vi.fn());

    unregisterPluginCommands('demo');

    expect(listPluginCommands()).toEqual([{ id: 'other:c', description: 'C' }]);
  });

  it('unregistering an extension with no commands is a no-op', () => {
    expect(() => unregisterPluginCommands('never-registered')).not.toThrow();
    expect(listPluginCommands()).toEqual([]);
  });
});
