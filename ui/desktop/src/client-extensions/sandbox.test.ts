/**
 * @vitest-environment jsdom
 */
import { describe, expect, it } from 'vitest';
import { buildSandboxedDocument, PLUGIN_FRAME_SANDBOX } from './sandbox';

function parse(html: string) {
  return new DOMParser().parseFromString(html, 'text/html');
}

const hostilePlugin =
  '<script>window.x="a&b"</script><meta http-equiv="Content-Security-Policy" content="default-src *">' +
  '</iframe><script>fetch("https://evil.example")</script><head><body onload="1">"quoted" & &amp; </script>';

describe('buildSandboxedDocument', () => {
  it('puts the content security policy before anything else in the outer document', () => {
    const doc = parse(buildSandboxedDocument(hostilePlugin));
    const first = doc.head.firstElementChild?.nextElementSibling;

    expect(first?.getAttribute('http-equiv')).toBe('Content-Security-Policy');
    expect(doc.head.querySelectorAll('meta[http-equiv]')).toHaveLength(1);
  });

  it('keeps hostile plugin html inside the inner frame and returns it unchanged', () => {
    const doc = parse(buildSandboxedDocument(hostilePlugin));

    expect(doc.querySelectorAll('iframe')).toHaveLength(1);
    expect(doc.querySelectorAll('script')).toHaveLength(1);
    expect(doc.querySelector('#plugin')?.getAttribute('srcdoc')).toBe(hostilePlugin);
  });

  it('denies every network channel and allows only what a self-contained plugin needs', () => {
    const csp = parse(buildSandboxedDocument('<p>hi</p>'))
      .querySelector('meta[http-equiv="Content-Security-Policy"]')
      ?.getAttribute('content');

    expect(csp).toContain("default-src 'none'");
    expect(csp).toContain("connect-src 'none'");
    expect(csp).toContain("frame-src 'none'");
    expect(csp).toContain("form-action 'none'");
    expect(csp).not.toMatch(/https?:|wss?:|\*/);
  });

  it('sandboxes the inner frame without same-origin, navigation or popup rights', () => {
    const doc = parse(buildSandboxedDocument('<p>hi</p>'));
    const sandbox = doc.querySelector('#plugin')?.getAttribute('sandbox');

    expect(sandbox).toBe(PLUGIN_FRAME_SANDBOX);
    expect(sandbox).toBe('allow-scripts');
  });
});
