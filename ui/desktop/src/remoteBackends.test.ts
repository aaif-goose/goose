import { describe, expect, it, vi } from 'vitest';
import { connectRemoteBackend, type RemoteBackendParams } from './remoteBackends';

type HopRequest = NonNullable<RemoteBackendParams['request']>;

const respond = (status: number, location?: string) => ({
  status,
  statusText: '',
  header: () => null,
  location: location ?? null,
});

describe('connectRemoteBackend', () => {
  it('checks /status and validates the secret against /acp', async () => {
    const request = vi.fn(async (url: string) => {
      if (url === 'https://example.com/goose/status') {
        return respond(200);
      }
      if (url === 'https://example.com/goose/acp') {
        return respond(406);
      }

      throw new Error(`Unexpected URL: ${url}`);
    });

    const result = await connectRemoteBackend({
      baseUrl: 'https://example.com/goose',
      serverSecret: 'test-secret',
      hasCertificatePin: false,
      request,
    });

    expect(result).toMatchObject({
      ok: true,
      failure: null,
      acpUrl: 'wss://example.com/goose/acp?token=test-secret',
    });
  });

  it('follows redirects and keeps the resolved ACP endpoint', async () => {
    const request = vi.fn(async (url: string) => {
      if (url === 'https://example.com/status') {
        return respond(302, 'https://backend.example.com/goose/status');
      }
      if (url === 'https://backend.example.com/goose/status') {
        return respond(200);
      }
      if (url === 'https://backend.example.com/goose/acp') {
        return respond(302, 'https://backend.example.com/socket?tenant=x');
      }
      if (url === 'https://backend.example.com/socket?tenant=x') {
        return respond(406);
      }

      throw new Error(`Unexpected URL: ${url}`);
    });

    const result = await connectRemoteBackend({
      baseUrl: 'https://example.com',
      serverSecret: 'test-secret',
      hasCertificatePin: false,
      request,
    });

    expect(result.ok).toBe(true);
    expect(result.acpUrl).toBe('wss://backend.example.com/socket?tenant=x&token=test-secret');
  });

  it('sends the secret only to the resolved ACP endpoint on the status origin', async () => {
    const request = vi.fn<HopRequest>(async (url) => {
      if (url === 'https://example.com/status') {
        return respond(200);
      }
      if (url === 'https://example.com/acp') {
        return respond(302, '/socket');
      }
      if (url === 'https://example.com/socket') {
        return respond(406);
      }

      throw new Error(`Unexpected URL: ${url}`);
    });

    const result = await connectRemoteBackend({
      baseUrl: 'https://example.com',
      serverSecret: 'test-secret',
      hasCertificatePin: true,
      request,
    });

    const secretRecipients = request.mock.calls
      .filter(([, init]) => init.headers?.['X-Secret-Key'])
      .map(([url]) => url);
    expect(result.ok).toBe(true);
    expect(secretRecipients).toEqual(['https://example.com/socket']);
  });

  it('does not follow redirects from the authenticated ACP request', async () => {
    const request = vi.fn<HopRequest>(async (url, init) => {
      if (url.endsWith('/status')) return respond(200);
      return init.headers ? respond(302, '/another-endpoint') : respond(401);
    });

    const result = await connectRemoteBackend({
      baseUrl: 'https://example.com',
      serverSecret: 'test-secret',
      hasCertificatePin: false,
      request,
    });

    expect(result.ok).toBe(false);
    expect(result.acpUrl).toBeNull();
    expect(request.mock.calls.map(([url]) => url)).toEqual([
      'https://example.com/status',
      'https://example.com/acp',
      'https://example.com/acp',
    ]);
  });

  it.each(['https://other.example.com/acp', 'https://example.com:8443/acp'])(
    'rejects a different ACP origin before sending the secret to %s',
    async (redirectUrl) => {
      const request = vi.fn<HopRequest>(async (url) =>
        url.endsWith('/status') ? respond(200) : respond(302, redirectUrl)
      );
      const result = await connectRemoteBackend({
        baseUrl: 'https://example.com',
        serverSecret: 'test-secret',
        hasCertificatePin: false,
        request,
      });

      expect(result.failure).toContain('outside the allowed backend origin');
      expect(request.mock.calls.map(([url]) => url)).toEqual([
        'https://example.com/status',
        'https://example.com/acp',
      ]);
      expect(request.mock.calls.every(([, init]) => !init.headers)).toBe(true);
    }
  );

  it('rejects an HTTPS to HTTP redirect', async () => {
    const request = vi.fn(async () => respond(302, 'http://backend.example.com/status'));

    const result = await connectRemoteBackend({
      baseUrl: 'https://example.com',
      serverSecret: 'test-secret',
      hasCertificatePin: false,
      request,
    });

    expect(result.ok).toBe(false);
    expect(result.failure).toContain('Redirect from HTTPS to HTTP is not allowed');
    expect(request).toHaveBeenCalledTimes(1);
  });

  it.each(['https://backend.example.com/status', 'https://example.com:8443/status'])(
    'rejects a status redirect outside the pinned origin to %s',
    async (redirectUrl) => {
      const request = vi.fn(async () => respond(302, redirectUrl));

      const result = await connectRemoteBackend({
        baseUrl: 'https://example.com',
        serverSecret: 'test-secret',
        hasCertificatePin: true,
        request,
      });

      expect(result.failure).toContain('outside the allowed backend origin');
      expect(request).toHaveBeenCalledTimes(1);
    }
  );

  it('reports the rejected secret without retrying', async () => {
    const request = vi.fn(async (url: string) => {
      if (url === 'https://example.com/status') {
        return respond(200);
      }
      if (url === 'https://example.com/acp') {
        return respond(401);
      }

      throw new Error(`Unexpected URL: ${url}`);
    });

    const result = await connectRemoteBackend({
      baseUrl: 'https://example.com',
      serverSecret: 'wrong-secret',
      hasCertificatePin: false,
      request,
    });

    expect(result.ok).toBe(false);
    expect(result.failure).toContain('Secret key: The backend rejected the secret key (HTTP 401)');
    expect(request).toHaveBeenCalledTimes(3);
  });

  it('reports an unusable URL without any request', async () => {
    const request = vi.fn();

    const result = await connectRemoteBackend({
      baseUrl: 'https://example.com/acp',
      serverSecret: 'test-secret',
      hasCertificatePin: false,
      request,
    });

    expect(result.ok).toBe(false);
    expect(result.failure).toContain('URL:');
    expect(request).not.toHaveBeenCalled();
  });

  it('stops retrying a fatal network failure', async () => {
    const request = vi.fn(async () => {
      throw new Error('net::ERR_NAME_NOT_RESOLVED');
    });

    const result = await connectRemoteBackend({
      baseUrl: 'https://nope.example.com',
      serverSecret: 'test-secret',
      hasCertificatePin: false,
      request,
    });

    expect(result.ok).toBe(false);
    expect(result.failure).toContain('ERR_NAME_NOT_RESOLVED');
    expect(request).toHaveBeenCalledTimes(1);
  });
});
