interface StoredMessage {
  content: string;
  timestamp: number;
}

export const CHAT_HISTORY_KEY = 'goose-chat-history';
const MAX_MESSAGES = 500;
const EXPIRY_DAYS = 30;

function readStoredMessages(stored: string | null): StoredMessage[] {
  if (!stored) return [];

  try {
    const messages = JSON.parse(stored) as StoredMessage[];
    const expiryTime = Date.now() - EXPIRY_DAYS * 24 * 60 * 60 * 1000;
    return messages.filter((msg) => msg.timestamp > expiryTime).slice(-MAX_MESSAGES);
  } catch (error) {
    console.error('Error reading message history:', error);
    return [];
  }
}

export function appendStoredMessage(stored: string | null, content: string): string | null {
  if (!content.trim()) return stored;

  const messages = readStoredMessages(stored);
  if (messages.at(-1)?.content === content) return stored;

  messages.push({ content, timestamp: Date.now() });
  return JSON.stringify(messages.slice(-MAX_MESSAGES));
}

export function pruneStoredMessageHistory(stored: string | null): string | null {
  return stored ? JSON.stringify(readStoredMessages(stored)) : null;
}

export class LocalMessageStorage {
  static addMessage(content: string) {
    window.electron.appendMessageHistory(content);
  }

  static getRecentMessages(): string[] {
    return readStoredMessages(window.electron.getRendererStorage(CHAT_HISTORY_KEY))
      .map((msg) => msg.content)
      .reverse();
  }

  static clearHistory() {
    window.electron.setRendererStorage(CHAT_HISTORY_KEY, null);
  }
}
