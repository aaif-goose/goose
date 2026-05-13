import { describe, it, expect } from 'vitest';
import { shouldShowNewChatTitle } from '../sessions';
import { getSessionDisplayName } from '../hooks/useNavigationSessions';
import type { Session } from '../api';

// Helper to build a minimal Session object for testing.
function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 'sess-1',
    name: 'untitled',
    message_count: 0,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    working_dir: '/tmp',
    extension_data: { active: [], installed: [] },
    ...overrides,
  };
}

describe('shouldShowNewChatTitle', () => {
  it('returns true for an empty session without a user-set name', () => {
    const session = makeSession({ message_count: 0, user_set_name: false });
    expect(shouldShowNewChatTitle(session)).toBe(true);
  });

  it('returns false when the session has messages', () => {
    const session = makeSession({ message_count: 3, user_set_name: false });
    expect(shouldShowNewChatTitle(session)).toBe(false);
  });

  it('returns false when the user has set a custom name', () => {
    const session = makeSession({ message_count: 0, user_set_name: true });
    expect(shouldShowNewChatTitle(session)).toBe(false);
  });

  it('returns false when the session has a recipe', () => {
    const session = makeSession({
      message_count: 0,
      user_set_name: false,
      recipe: { title: 'Recipe', steps: [] } as unknown as Session['recipe'],
    });
    expect(shouldShowNewChatTitle(session)).toBe(false);
  });
});

describe('session reuse scoping (fix for #7601)', () => {
  // Simulates the core logic extracted from handleNewChat in useNavigationSessions.ts.
  // Before the fix: `sessions.find(s => shouldShowNewChatTitle(s))` picked the
  // first global empty session regardless of which window called it.
  // After the fix: only the current window's activeSessionId is considered.
  function findReusableSession(
    sessions: Session[],
    activeSessionId: string | undefined
  ): Session | undefined {
    const currentActive = activeSessionId
      ? sessions.find((s) => s.id === activeSessionId)
      : undefined;
    if (currentActive && shouldShowNewChatTitle(currentActive)) {
      return currentActive;
    }
    return undefined;
  }

  const emptySessionA = makeSession({ id: 'empty-a', message_count: 0, user_set_name: false });
  const emptySessionB = makeSession({ id: 'empty-b', message_count: 0, user_set_name: false });
  const usedSession = makeSession({ id: 'used-c', message_count: 5, user_set_name: true });

  const allSessions = [emptySessionA, emptySessionB, usedSession];

  it("window A only reuses its own active empty session, not window B's", () => {
    // Window A has emptySessionA active, Window B has emptySessionB active.
    // Under the old logic, both would grab emptySessionA (the first in the list).
    const windowAResult = findReusableSession(allSessions, 'empty-a');
    const windowBResult = findReusableSession(allSessions, 'empty-b');

    expect(windowAResult?.id).toBe('empty-a');
    expect(windowBResult?.id).toBe('empty-b');
    // They never collide on the same session.
    expect(windowAResult?.id).not.toBe(windowBResult?.id);
  });

  it('does not reuse a session that has messages even if it is active', () => {
    const result = findReusableSession(allSessions, 'used-c');
    expect(result).toBeUndefined();
  });

  it('returns undefined when there is no active session id', () => {
    const result = findReusableSession(allSessions, undefined);
    expect(result).toBeUndefined();
  });

  it('returns undefined when the active session id is not in the list', () => {
    const result = findReusableSession(allSessions, 'nonexistent');
    expect(result).toBeUndefined();
  });

  it('demonstrates the old bug: global find would give same session to both windows', () => {
    // Old logic (before fix) - both windows get the same session.
    const oldLogicFind = (sessions: Session[]) => sessions.find((s) => shouldShowNewChatTitle(s));

    const windowAOld = oldLogicFind(allSessions);
    const windowBOld = oldLogicFind(allSessions);

    // Both windows would grab the exact same session - the bug.
    expect(windowAOld?.id).toBe(windowBOld?.id);
    expect(windowAOld?.id).toBe('empty-a');
  });
});

describe('getSessionDisplayName (fix for #8865)', () => {
  it('returns the user-set name for a recipe session that has been renamed', () => {
    const session = makeSession({
      name: 'My Renamed Chat',
      user_set_name: true,
      message_count: 2,
      recipe: { title: 'Some Recipe' } as unknown as Session['recipe'],
    });
    expect(getSessionDisplayName(session)).toBe('My Renamed Chat');
  });

  it('falls back to the recipe title when the user has not renamed', () => {
    const session = makeSession({
      name: 'auto-generated',
      user_set_name: false,
      message_count: 2,
      recipe: { title: 'Some Recipe' } as unknown as Session['recipe'],
    });
    expect(getSessionDisplayName(session)).toBe('Some Recipe');
  });
});
