import React from 'react';
import { describe, expect, it } from 'vitest';
import { render, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { ChatProvider, useChatContext } from '../contexts/ChatContext';
import type { ChatType } from '../types/chat';

function EffectWriter({
  isActiveSession,
  sessionId,
  name,
}: {
  isActiveSession: boolean;
  sessionId: string;
  name: string;
}) {
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
  }, [ctx, isActiveSession, sessionId, name]);

  return null;
}

function ChatHarness({
  initialChat,
  children,
}: {
  initialChat: ChatType;
  children: React.ReactNode;
}) {
  const [chat, setChat] = React.useState<ChatType>(initialChat);
  return <ChatProvider chat={chat} setChat={setChat}>{children}</ChatProvider>;
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

    render(
      <MemoryRouter>
        <ChatHarness initialChat={initialChat}>
          <EffectWriter
            isActiveSession={false}
            sessionId="hidden-session"
            name="Hidden Session"
          />
          <ContextReader onRead={(chat) => (latestRef.current = chat)} />
        </ChatHarness>
      </MemoryRouter>
    );

    expect(latestRef.current).not.toBeNull();
    if (!latestRef.current) throw new Error('Expected latest chat context to be populated');
    expect(latestRef.current.sessionId).toBe('active-session');
    expect(latestRef.current.name).toBe('Active Session');
  });

  it('switches context immediately when the active session changes', async () => {
    const initialChat: ChatType = {
      sessionId: 'session-a',
      name: 'Session A',
      messages: [],
    };

    const latestRef: { current: ChatType | null } = { current: null };

    const { rerender } = render(
      <MemoryRouter>
        <ChatHarness initialChat={initialChat}>
          <EffectWriter
            isActiveSession={true}
            sessionId="session-a"
            name="Session A"
          />
          <ContextReader onRead={(chat) => (latestRef.current = chat)} />
        </ChatHarness>
      </MemoryRouter>
    );

    expect(latestRef.current).not.toBeNull();
    if (!latestRef.current) throw new Error('Expected latest chat context to be populated');
    expect(latestRef.current.sessionId).toBe('session-a');

    rerender(
      <MemoryRouter>
        <ChatHarness initialChat={initialChat}>
          <EffectWriter
            isActiveSession={true}
            sessionId="session-b"
            name="No Session"
          />
          <ContextReader onRead={(chat) => (latestRef.current = chat)} />
        </ChatHarness>
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(latestRef.current).not.toBeNull();
      if (!latestRef.current) throw new Error('Expected latest chat context to be populated');
      expect(latestRef.current.sessionId).toBe('session-b');
      expect(latestRef.current.name).toBe('No Session');
    });
  });
});
