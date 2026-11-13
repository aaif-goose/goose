import { describe, expect, it } from 'vitest';
import { extractCodeBlocks, extractCodeLanguages, stripCodeBlocksForLanguage } from './messageContext';

describe('extractCodeBlocks', () => {
  it('extracts fenced code blocks with language tags', () => {
    const text = 'Intro\n```json\n{"a":1}\n```\nOutro';
    expect(extractCodeBlocks(text)).toEqual([{ language: 'json', content: '{"a":1}' }]);
  });

  it('collects unique languages', () => {
    const text = '```js\n1\n```\n```json\n{}\n```\n```js\n2\n```';
    expect(extractCodeLanguages(text)).toEqual(['js', 'json']);
  });

  it('strips matched code blocks for custom render', () => {
    const text = 'Here is data:\n```json\n{"a":1}\n```\nDone.';
    expect(stripCodeBlocksForLanguage(text, 'json')).toBe('Here is data:\n\nDone.');
  });
});
