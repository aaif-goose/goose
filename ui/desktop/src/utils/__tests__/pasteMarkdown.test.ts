import { describe, it, expect } from 'vitest';
import { htmlToMarkdown } from '../pasteMarkdown';

describe('htmlToMarkdown', () => {
  it('returns null when there are no links', () => {
    expect(htmlToMarkdown('<p>plain text</p>')).toBeNull();
  });

  it('converts a simple link', () => {
    expect(htmlToMarkdown('<a href="https://example.com">Example</a>')).toBe(
      '[Example](https://example.com)'
    );
  });

  it('converts text with an inline link', () => {
    expect(htmlToMarkdown('<p>Visit <a href="https://example.com">Example</a> today</p>')).toBe(
      'Visit [Example](https://example.com) today'
    );
  });

  it('converts multiple links', () => {
    const html = '<p><a href="https://a.com">A</a> and <a href="https://b.com">B</a></p>';
    expect(htmlToMarkdown(html)).toBe('[A](https://a.com) and [B](https://b.com)');
  });

  it('preserves paragraph breaks', () => {
    const html = '<p><a href="https://a.com">A</a></p><p><a href="https://b.com">B</a></p>';
    expect(htmlToMarkdown(html)).toBe('[A](https://a.com)\n\n[B](https://b.com)');
  });

  it('converts BR tags to newlines', () => {
    const html = '<a href="https://a.com">A</a><br><a href="https://b.com">B</a>';
    expect(htmlToMarkdown(html)).toBe('[A](https://a.com)\n[B](https://b.com)');
  });

  it('uses href as text when link text is empty', () => {
    expect(htmlToMarkdown('<a href="https://example.com"></a>')).toBe(
      '[https://example.com](https://example.com)'
    );
  });

  it('strips style tags from Google Docs paste', () => {
    const html =
      '<html><head><style>.c0{font-weight:bold}</style></head><body>' +
      '<p>Check <a href="https://example.com">this</a></p></body></html>';
    expect(htmlToMarkdown(html)).toBe('Check [this](https://example.com)');
  });

  it('strips script tags', () => {
    const html =
      '<script>alert("xss")</script><p>See <a href="https://example.com">link</a></p>';
    expect(htmlToMarkdown(html)).toBe('See [link](https://example.com)');
  });

  it('strips meta and title tags from Office paste', () => {
    const html =
      '<html><head><meta charset="utf-8"><title>Doc</title></head><body>' +
      '<p><a href="https://example.com">link</a></p></body></html>';
    expect(htmlToMarkdown(html)).toBe('[link](https://example.com)');
  });

  it('strips SVG elements', () => {
    const html =
      '<svg><text>icon</text></svg><a href="https://example.com">link</a>';
    expect(htmlToMarkdown(html)).toBe('[link](https://example.com)');
  });

  it('handles nested spans inside links', () => {
    const html = '<a href="https://example.com"><span class="c1">styled link</span></a>';
    expect(htmlToMarkdown(html)).toBe('[styled link](https://example.com)');
  });

  it('handles div containers', () => {
    const html = '<div>Hello <a href="https://example.com">world</a></div>';
    expect(htmlToMarkdown(html)).toBe('Hello [world](https://example.com)');
  });

  it('collapses excessive newlines', () => {
    const html =
      '<p><a href="https://a.com">A</a></p><p></p><p></p><p><a href="https://b.com">B</a></p>';
    expect(htmlToMarkdown(html)).toBe('[A](https://a.com)\n\n[B](https://b.com)');
  });
});
