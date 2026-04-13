import React from 'react';
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { ChatProvider, useChatContext } from '../contexts/ChatContext';
import type { ChatType } from '../types/chat';

function setChatFromEffect({
  isActiveSession,
  sessionId,
  name,
}: {
  isActiveSession: boolean;
  sessionId: string;
  name: string;
}) {
  return function EffectWriter() {
    const ctx = useChatContext();

    React.useEffect(() => {
      if (!ctx) return;
      // Mirrors BaseChat behavior after fix: hidden sessions don't update global chat context.
      if (!isActiveSession) return;
      ctx.setChat({
        sessionId,
        name,
        messages: [],
      });
    }, [ctx]);

    return null;
  };
}

function ContextReader({ onRead }: { onRead: (chat: ChatType | null) => void }) {
  const ctx = useChatContext();

  React.useEffect(() => {
    onRead(ctx?.chat ?? null);
  }, [ctx?.chat?.sessionId, ctx?.chat?.name]);

  return null;
}

describe('chat context isolation for background sessions', () => {
  it('does not let hidden/background sessions overwrite global chat context', async () => {
    const initialChat: ChatType = {
      sessionId: 'active-session',
      name: 'Active Session',
      messages: [],
    };

    const latestRef: { current: ChatType | null } = { current: null };

    const HiddenWriter = setChatFromEffect({
      isActiveSession: false,
      sessionId: 'hidden-session',
      name: 'Hidden Session',
    });

    render(
      <MemoryRouter>
        <ChatProvider chat={initialChat} setChat={() => {}}>
          <HiddenWriter />
          <ContextReader onRead={(chat) => (latestRef.current = chat)} />
        </ChatProvider>
      </MemoryRouter>
    );

    expect(latestRef.current).not.toBeNull();
    if (!latestRef.current) throw new Error('Expected latest chat context to be populated');
    expect(latestRef.current.sessionId).toBe('active-session');
    expect(latestRef.current.name).toBe('Active Session');
  });
});
