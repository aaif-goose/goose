import { describe, expect, it } from 'vitest';
import { resolveModelProvider } from './sessionModelProvider';

describe('resolveModelProvider', () => {
  it('uses override first', () => {
    const result = resolveModelProvider({
      sessionId: 'sess-1',
      sessionModel: 'claude-3-haiku',
      sessionProvider: 'anthropic',
      configModel: 'gpt-4.1',
      configProvider: 'openai',
      override: { model: 'o3', provider: 'openai' },
    });

    expect(result).toEqual({ model: 'o3', provider: 'openai', isSessionScoped: true });
  });

  it('uses session-scoped model/provider when session is active', () => {
    const result = resolveModelProvider({
      sessionId: 'sess-1',
      sessionModel: 'claude-3-opus',
      sessionProvider: 'anthropic',
      configModel: 'gpt-4.1',
      configProvider: 'openai',
      override: null,
    });

    expect(result).toEqual({
      model: 'claude-3-opus',
      provider: 'anthropic',
      isSessionScoped: true,
    });
  });

  it('does not fall back to global config while session model/provider are loading', () => {
    const result = resolveModelProvider({
      sessionId: 'sess-1',
      sessionModel: null,
      sessionProvider: null,
      configModel: 'gpt-4.1',
      configProvider: 'openai',
      override: null,
    });

    expect(result).toEqual({ model: null, provider: null, isSessionScoped: true });
  });

  it('falls back to global config when no session is active', () => {
    const result = resolveModelProvider({
      sessionId: null,
      sessionModel: null,
      sessionProvider: null,
      configModel: 'gpt-4.1',
      configProvider: 'openai',
      override: null,
    });

    expect(result).toEqual({ model: 'gpt-4.1', provider: 'openai', isSessionScoped: false });
  });
});
