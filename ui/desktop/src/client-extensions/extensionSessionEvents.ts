export type ExtensionSessionEvent =
  | {
      type: 'agent_message_chunk';
      sessionId: string;
      content: unknown;
    }
  | {
      type: 'tool_call';
      sessionId: string;
      toolCallId: string;
      title?: string | null;
      status?: string | null;
    }
  | {
      type: 'status_message';
      sessionId: string;
      message: string;
      level: 'notice' | 'progress';
    };

type ExtensionSessionEventListener = (event: ExtensionSessionEvent) => void;

const listeners = new Set<ExtensionSessionEventListener>();

export function subscribeExtensionSessionEvents(
  listener: ExtensionSessionEventListener
): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function publishExtensionSessionEvent(event: ExtensionSessionEvent): void {
  for (const listener of [...listeners]) {
    try {
      listener(event);
    } catch (error) {
      console.error('[client-extensions] Session event listener failed:', error);
    }
  }
}
