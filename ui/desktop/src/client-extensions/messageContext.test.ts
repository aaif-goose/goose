import { describe, expect, it } from 'vitest';
import {
  extractCodeBlocks,
  extractCodeLanguages,
  stripFirstCodeBlockForLanguage,
} from './messageContext';

describe('extractCodeBlocks', () => {
  it('extracts fenced code blocks with language tags', () => {
    const text = 'Intro\n```json\n{"a":1}\n```\nOutro';
    expect(extractCodeBlocks(text)).toEqual([{ language: 'json', content: '{"a":1}' }]);
  });

  it('collects unique languages', () => {
    const text = '```js\n1\n```\n```json\n{}\n```\n```js\n2\n```';
    expect(extractCodeLanguages(text)).toEqual(['js', 'json']);
  });
});

describe('stripFirstCodeBlockForLanguage', () => {
  it('strips the matched code block for custom render', () => {
    const text = 'Here is data:\n```json\n{"a":1}\n```\nDone.';
    expect(stripFirstCodeBlockForLanguage(text, 'json')).toBe('Here is data:\n\nDone.');
  });

  it('leaves later blocks of the same language in place', () => {
    const text = 'A\n```json\n{"a":1}\n```\nB\n```json\n{"b":2}\n```\nC';
    expect(stripFirstCodeBlockForLanguage(text, 'json')).toBe('A\n\nB\n```json\n{"b":2}\n```\nC');
  });

  it('ignores blocks of other languages', () => {
    const text = '```js\n1\n```';
    expect(stripFirstCodeBlockForLanguage(text, 'json')).toBe(text);
  });
});
