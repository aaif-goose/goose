import type { ExternalBackendConfig } from './settings';

const DEFAULT_CONNECT_SOURCES = [
  "'self'",
  'http://127.0.0.1:*',
  'https://127.0.0.1:*',
  'ws://127.0.0.1:*',
  'wss://127.0.0.1:*',
  'http://localhost:*',
  'https://localhost:*',
  'ws://localhost:*',
  'wss://localhost:*',
  'https://api.github.com',
  'https://github.com',
  'https://objects.githubusercontent.com',
];

export function buildConnectSrc(externalBackend?: ExternalBackendConfig): string {
  const sources = [...DEFAULT_CONNECT_SOURCES];

  if (externalBackend?.enabled && externalBackend.url) {
    try {
      const externalUrl = new URL(externalBackend.url);
      sources.push(externalUrl.origin);
      externalUrl.protocol = externalUrl.protocol === 'https:' ? 'wss:' : 'ws:';
      sources.push(externalUrl.origin);
    } catch {
      console.warn('Invalid external backend URL in settings, skipping CSP entry');
    }
  }

  return sources.join(' ');
}

/**
 * Returns true when upgrade-insecure-requests should be included in the CSP.
 *
 * The directive is omitted when the user has configured an external backend
 * that uses plain HTTP, because Chromium would silently rewrite those
 * requests to HTTPS. The remote server typically does not speak TLS, so the
 * upgraded requests fail with "Failed to fetch".
 *
 * Loopback addresses (127.0.0.1 / localhost) are exempt from the upgrade
 * per the CSP spec, which is why the built-in local backend is unaffected.
 */
export function shouldUpgradeInsecureRequests(externalBackend?: ExternalBackendConfig): boolean {
  if (!externalBackend?.enabled || !externalBackend.url) {
    return true;
  }

  try {
    const parsed = new URL(externalBackend.url);
    return parsed.protocol !== 'http:';
  } catch {
    return true;
  }
}

export function buildCSP(externalBackend?: ExternalBackendConfig): string {
  const connectSrc = buildConnectSrc(externalBackend);
  const upgradeDirective = shouldUpgradeInsecureRequests(externalBackend)
    ? 'upgrade-insecure-requests;'
    : '';

  return (
    "default-src 'self';" +
    "style-src 'self' 'unsafe-inline';" +
    "script-src 'self' 'unsafe-inline';" +
    "img-src 'self' data: https:;" +
    `connect-src ${connectSrc};` +
    "object-src 'none';" +
    "frame-src 'self' https: http:;" +
    "font-src 'self' data: https:;" +
    "media-src 'self' mediastream:;" +
    "form-action 'none';" +
    "base-uri 'self';" +
    "manifest-src 'self';" +
    "worker-src 'self';" +
    upgradeDirective
  );
}
