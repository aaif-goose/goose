import { net } from 'electron';
import type { HopRequest, HopResponse } from './backendRedirects';

const headerValue = (headers: Record<string, string | string[]>, name: string): string | null => {
  const value = headers[name.toLowerCase()];
  if (value === undefined) {
    return null;
  }
  return Array.isArray(value) ? (value[0] ?? null) : value;
};

// net.fetch reports an incorrect Response.url, so each hop is issued separately
// with net.request and the caller decides whether to follow it.
export const netHopRequest: HopRequest = (url, init) =>
  new Promise<HopResponse>((resolve, reject) => {
    const request = net.request({ method: 'GET', url, redirect: 'manual', credentials: 'omit' });
    let settled = false;

    const abort = () => request.abort();
    init.signal?.addEventListener('abort', abort, { once: true });

    const settle = (finish: () => void) => {
      if (settled) {
        return;
      }
      settled = true;
      init.signal?.removeEventListener('abort', abort);
      finish();
    };

    // A manual redirect is not followed, so the request is cancelled after this
    // event and only the 'abort' event follows.
    request.on('redirect', (statusCode, _method, redirectUrl, responseHeaders) => {
      settle(() =>
        resolve({
          status: statusCode,
          statusText: '',
          headers: { get: (name) => headerValue(responseHeaders, name) },
          location: redirectUrl,
        })
      );
      request.abort();
    });

    request.on('response', (response) => {
      response.on('data', () => undefined);
      response.on('end', () =>
        settle(() =>
          resolve({
            status: response.statusCode,
            statusText: response.statusMessage,
            headers: { get: (name) => headerValue(response.headers, name) },
            location: null,
          })
        )
      );
      response.on('error', (error: Error) => settle(() => reject(error)));
    });

    request.on('error', (error) => settle(() => reject(error)));
    request.on('abort', () => settle(() => reject(new Error('Request aborted'))));

    for (const [name, value] of Object.entries(init.headers ?? {})) {
      request.setHeader(name, value);
    }
    request.end();
  });
