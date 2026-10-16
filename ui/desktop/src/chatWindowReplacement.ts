import type { App, BrowserWindow } from 'electron';

type ChatWindow = Pick<
  BrowserWindow,
  'id' | 'isDestroyed' | 'isVisible' | 'once' | 'removeListener'
>;

export class ChatWindowReplacer {
  private pending = new Map<string, Promise<void>>();

  replace(
    previousWindow: ChatWindow,
    sessionId: string,
    createWindow: () => Promise<ChatWindow | undefined>
  ): Promise<void> {
    const key = JSON.stringify([previousWindow.id, sessionId]);
    const existing = this.pending.get(key);
    if (existing) return existing;
    const replacement = this.openReplacement(createWindow).finally(() => {
      this.pending.delete(key);
    });
    this.pending.set(key, replacement);
    return replacement;
  }

  private async openReplacement(
    createWindow: () => Promise<ChatWindow | undefined>
  ): Promise<void> {
    const nextWindow = await createWindow();
    if (!nextWindow || nextWindow.isDestroyed()) throw new Error('Failed to create chat window');
    if (!nextWindow.isVisible()) {
      await new Promise<void>((resolve, reject) => {
        const cleanup = () => {
          nextWindow.removeListener('ready-to-show', ready);
          nextWindow.removeListener('closed', closed);
        };
        const ready = () => {
          cleanup();
          resolve();
        };
        const closed = () => {
          cleanup();
          reject(new Error('Failed to create chat window'));
        };
        nextWindow.once('ready-to-show', ready);
        nextWindow.once('closed', closed);
      });
    }
  }
}

export function handleChatWindowStartupFailure(
  app: Pick<App, 'quit'>,
  windows: readonly unknown[]
): void {
  if (windows.length === 0) app.quit();
}
