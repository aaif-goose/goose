import { describe, expect, it } from 'vitest';
import { matchesCustomRender, selectCustomRender } from './customRender';
import type { MessageExtensionHostContext, RegisteredCustomRender } from './types';

const baseContext: MessageExtensionHostContext = {
  sessionId: 's1',
  route: '/pair',
  messageId: 'm1',
  role: 'assistant',
  hasText: true,
  hasImage: false,
  hasToolRequests: false,
  codeLanguages: ['json'],
};

describe('matchesCustomRender', () => {
  it('matches language-specific code blocks', () => {
    expect(
      matchesCustomRender(
        { contentType: 'code', language: 'json' },
        baseContext,
        [{ language: 'json', content: '{}' }]
      )
    ).toBe(true);
    expect(
      matchesCustomRender(
        { contentType: 'code', language: 'mermaid' },
        baseContext,
        [{ language: 'json', content: '{}' }]
      )
    ).toBe(false);
  });
});

describe('selectCustomRender', () => {
  it('picks the highest priority matching render', () => {
    const renders: RegisteredCustomRender[] = [
      {
        id: 'low',
        extensionId: 'ext-a',
        match: { language: 'json' },
        priority: 1,
      },
      {
        id: 'high',
        extensionId: 'ext-b',
        match: { language: 'json' },
        priority: 50,
      },
    ];

    const selected = selectCustomRender(renders, baseContext, [
      { language: 'json', content: '{"x":1}' },
    ]);

    expect(selected?.id).toBe('high');
  });
});
