import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('preload file access boundary', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it('exposes only narrow file operations without renderer-supplied paths', async () => {
    const exposed: Record<string, unknown> = {};
    const invoke = vi.fn();
    vi.doMock('electron', () => ({
      default: {},
      contextBridge: {
        exposeInMainWorld: (name: string, api: unknown) => {
          exposed[name] = api;
        },
      },
      ipcRenderer: {
        emit: vi.fn(),
        invoke,
        off: vi.fn(),
        on: vi.fn(),
        removeListener: vi.fn(),
        send: vi.fn(),
        sendSync: vi.fn(),
      },
      webUtils: { getPathForFile: vi.fn() },
    }));

    await import('./preload');

    const electron = exposed.electron as Record<string, (...args: unknown[]) => unknown>;
    expect(electron).not.toHaveProperty('readFile');

    electron.selectRecipeFile('/etc/passwd');
    electron.readGoosehints('../secret');
    electron.writeGoosehints('project guidance', '../secret');

    expect(invoke).toHaveBeenNthCalledWith(1, 'select-recipe-file');
    expect(invoke).toHaveBeenNthCalledWith(2, 'read-goosehints');
    expect(invoke).toHaveBeenNthCalledWith(3, 'write-goosehints', 'project guidance');
  });
  it('awaits window replacement through IPC', async () => {
    const exposed: Record<string, unknown> = {};
    const invoke = vi.fn().mockResolvedValue(undefined);
    vi.doMock('electron', () => ({
      default: {},
      contextBridge: {
        exposeInMainWorld: (name: string, api: unknown) => {
          exposed[name] = api;
        },
      },
      ipcRenderer: {
        emit: vi.fn(),
        invoke,
        off: vi.fn(),
        on: vi.fn(),
        removeListener: vi.fn(),
        send: vi.fn(),
        sendSync: vi.fn(),
      },
      webUtils: { getPathForFile: vi.fn() },
    }));
    await import('./preload');
    const electron = exposed.electron as {
      replaceChatWindow(options: { dir: string; resumeSessionId: string }): Promise<void>;
    };
    const options = { dir: '/replacement', resumeSessionId: 'session-1' };
    await electron.replaceChatWindow(options);
    expect(invoke).toHaveBeenCalledWith('replace-chat-window', options);
  });
});
