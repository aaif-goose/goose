import { acpHttpUrlFromHttpBase, normalizeAcpHttpBaseUrl } from './acp/url';

// The connection is tested by opening the real ACP WebSocket from the renderer,
// so this only has to derive the URL that socket connects to.
export const acpWebSocketUrl = (baseUrl: string, secret: string): string => {
  const url = new URL(acpHttpUrlFromHttpBase(normalizeAcpHttpBaseUrl(baseUrl)));
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.hash = '';
  url.searchParams.set('token', secret);
  return url.toString();
};
