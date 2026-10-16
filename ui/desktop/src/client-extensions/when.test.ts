import { describe, expect, it } from 'vitest';
import { evaluateWhenClause } from './when';
import type { MessageExtensionHostContext } from './types';

describe('evaluateWhenClause', () => {
  it('defaults to visible when clause is missing', () => {
    expect(evaluateWhenClause(undefined, { sessionId: null, route: '/' })).toBe(true);
  });

  it('matches session.active', () => {
    expect(
      evaluateWhenClause('session.active', { sessionId: 'abc', route: '/pair' })
    ).toBe(true);
    expect(evaluateWhenClause('session.active', { sessionId: null, route: '/' })).toBe(false);
  });

  it('matches !session.active', () => {
    expect(evaluateWhenClause('!session.active', { sessionId: null, route: '/' })).toBe(true);
    expect(
      evaluateWhenClause('!session.active', { sessionId: 'abc', route: '/pair' })
    ).toBe(false);
  });

  const messageContext: MessageExtensionHostContext = {
    sessionId: 'abc',
    route: '/pair',
    messageId: 'm1',
    role: 'assistant',
    hasText: true,
    hasImage: true,
    hasToolRequests: false,
    codeLanguages: ['json', 'mermaid'],
  };

  it('matches message.hasImage', () => {
    expect(evaluateWhenClause('message.hasImage', messageContext)).toBe(true);
    const withoutImage: MessageExtensionHostContext = { ...messageContext, hasImage: false };
    expect(evaluateWhenClause('message.hasImage', withoutImage)).toBe(false);
  });

  it('matches message.codeLanguage.*', () => {
    expect(evaluateWhenClause('message.codeLanguage.json', messageContext)).toBe(true);
    expect(evaluateWhenClause('message.codeLanguage.rust', messageContext)).toBe(false);
  });

  it('returns false for message clauses without message context', () => {
    expect(evaluateWhenClause('message.hasImage', { sessionId: 'abc', route: '/' })).toBe(false);
  });

  it('matches route.pair', () => {
    expect(evaluateWhenClause('route.pair', { sessionId: null, route: '/pair' })).toBe(true);
    expect(evaluateWhenClause('route.pair', { sessionId: null, route: '/settings' })).toBe(false);
  });
});
