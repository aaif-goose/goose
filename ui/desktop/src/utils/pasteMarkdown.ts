const NON_CONTENT_TAGS = new Set([
  'STYLE',
  'SCRIPT',
  'NOSCRIPT',
  'HEAD',
  'META',
  'LINK',
  'TITLE',
  'TEMPLATE',
  'SVG',
  'MATH',
  'IFRAME',
  'OBJECT',
  'EMBED',
  'APPLET',
  'COMMENT',
]);

function convertNodeToMarkdown(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) {
    return node.textContent || '';
  }
  if (node.nodeType === Node.ELEMENT_NODE) {
    const el = node as HTMLElement;
    const tag = el.tagName.toUpperCase();
    if (NON_CONTENT_TAGS.has(tag)) {
      return '';
    }
    if (tag === 'A' && el.getAttribute('href')) {
      const href = el.getAttribute('href')!;
      const text = el.textContent || href;
      return `[${text}](${href})`;
    }
    if (tag === 'BR') {
      return '\n';
    }
    if (tag === 'P' || tag === 'DIV') {
      const inner = Array.from(el.childNodes).map(convertNodeToMarkdown).join('');
      return inner + '\n\n';
    }
    return Array.from(el.childNodes).map(convertNodeToMarkdown).join('');
  }
  return '';
}

/**
 * Converts pasted HTML containing hyperlinks into markdown text.
 * Returns null if the HTML has no links (caller should let the browser handle the paste).
 */
export function htmlToMarkdown(html: string): string | null {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');
  if (doc.querySelectorAll('a[href]').length === 0) {
    return null;
  }
  return convertNodeToMarkdown(doc.body).replace(/\n{3,}/g, '\n\n').trim();
}
