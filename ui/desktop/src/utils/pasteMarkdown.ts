import TurndownService from 'turndown';

const turndown = new TurndownService({
  headingStyle: 'atx',
  bulletListMarker: '-',
  codeBlockStyle: 'fenced',
});

export function htmlToMarkdown(html: string): string | null {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');
  if (doc.querySelectorAll('a[href]').length === 0) {
    return null;
  }
  return turndown.turndown(doc.body).trim();
}
