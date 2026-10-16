import { EventEmitter } from 'node:events';
import { describe, expect, it, vi } from 'vitest';
import { ChatWindowReplacer, handleChatWindowStartupFailure } from './chatWindowReplacement';

type ChatWindow = Parameters<ChatWindowReplacer['replace']>[0];
function windowFixture(id: number, visible = false) {
  const events = new EventEmitter();
  const window = Object.assign(events, {
    id,
    isDestroyed: vi.fn(() => false),
    isVisible: () => visible,
    close: vi.fn(),
  });
  return { window, value: window as unknown as ChatWindow };
}

describe('ChatWindowReplacer', () => {
  it('waits for the replacement and leaves closing the original window to its renderer', async () => {
    const old = windowFixture(1);
    const next = windowFixture(2);
    const create = vi.fn().mockResolvedValue(next.value);
    const replacer = new ChatWindowReplacer();
    const first = replacer.replace(old.value, 'session-1', create);
    const duplicate = replacer.replace(old.value, 'session-1', create);
    expect(first).toBe(duplicate);
    await vi.waitFor(() => expect(next.window.listenerCount('ready-to-show')).toBe(1));
    expect(old.window.close).not.toHaveBeenCalled();
    next.window.emit('ready-to-show');
    await first;
    expect(create).toHaveBeenCalledOnce();
    expect(old.window.close).not.toHaveBeenCalled();
    expect(next.window.listenerCount('closed')).toBe(0);
  });

  it('opens separate windows for concurrent handoffs of different sessions', async () => {
    const old = windowFixture(1);
    const first = windowFixture(2, true);
    const second = windowFixture(3, true);
    const createFirst = vi.fn().mockResolvedValue(first.value);
    const createSecond = vi.fn().mockResolvedValue(second.value);
    const replacer = new ChatWindowReplacer();
    await Promise.all([
      replacer.replace(old.value, 'session-1', createFirst),
      replacer.replace(old.value, 'session-2', createSecond),
    ]);
    expect(createFirst).toHaveBeenCalledOnce();
    expect(createSecond).toHaveBeenCalledOnce();
    expect(old.window.close).not.toHaveBeenCalled();
  });

  it('uses an already visible replacement', async () => {
    const old = windowFixture(1);
    await new ChatWindowReplacer().replace(
      old.value,
      'session-1',
      async () => windowFixture(2, true).value
    );
    expect(old.window.close).not.toHaveBeenCalled();
  });

  it.each(['missing', 'closed', 'throwing'])(
    'preserves the original window and permits retry after a %s replacement',
    async (failure) => {
      const old = windowFixture(1);
      const next = windowFixture(2);
      const replacer = new ChatWindowReplacer();
      const replacement = replacer.replace(old.value, 'session-1', async () => {
        if (failure === 'throwing') throw new Error('Startup failed');
        return failure === 'missing' ? undefined : next.value;
      });
      const rejection = expect(replacement).rejects.toThrow();
      if (failure === 'closed') {
        await vi.waitFor(() => expect(next.window.listenerCount('closed')).toBe(1));
        next.window.emit('closed');
      }
      await rejection;
      expect(old.window.close).not.toHaveBeenCalled();
      await replacer.replace(old.value, 'session-1', async () => windowFixture(3, true).value);
      expect(old.window.close).not.toHaveBeenCalled();
    }
  );
});

describe('chat window backend startup failure', () => {
  it('keeps an existing recovery window and permits another replacement attempt', async () => {
    const app = { quit: vi.fn() };
    const old = windowFixture(1);
    const replacer = new ChatWindowReplacer();
    await expect(
      replacer.replace(old.value, 'session-1', async () => {
        handleChatWindowStartupFailure(app, [old.value]);
        return undefined;
      })
    ).rejects.toThrow('Failed to create chat window');
    expect(app.quit).not.toHaveBeenCalled();
    expect(old.window.close).not.toHaveBeenCalled();
    await replacer.replace(old.value, 'session-1', async () => windowFixture(2, true).value);
    expect(old.window.close).not.toHaveBeenCalled();
  });

  it('retains the existing cold-start failure exit when there is no window', () => {
    const app = { quit: vi.fn() };
    handleChatWindowStartupFailure(app, []);
    expect(app.quit).toHaveBeenCalledOnce();
  });
});
