import { describe, it, expect, vi, beforeEach } from 'vitest';
import { EventEmitter } from 'events';
import type { IncomingMessage, ClientRequest, RequestOptions } from 'node:http';

type GetFn = (
  url: string | URL,
  options: RequestOptions,
  cb: (res: IncomingMessage) => void
) => ClientRequest;

const { mockHttpsGet } = vi.hoisted(() => ({ mockHttpsGet: vi.fn<GetFn>() }));

vi.mock('node:https', () => ({
  default: { get: mockHttpsGet },
}));

import { fetchAllowlistContent, parseAllowlistCommands } from './allowlistFetcher';

type MockResponse = EventEmitter & {
  statusCode: number;
  headers: Record<string, string>;
  setEncoding: ReturnType<typeof vi.fn>;
  resume: ReturnType<typeof vi.fn>;
};

function mockResponse(statusCode: number, headers: Record<string, string> = {}): MockResponse {
  const res = new EventEmitter() as MockResponse;
  res.statusCode = statusCode;
  res.headers = headers;
  res.setEncoding = vi.fn();
  res.resume = vi.fn();
  return res;
}

function mockRequest(): EventEmitter {
  return new EventEmitter();
}

describe('fetchAllowlistContent', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('rejects an HTTP initial URL without making a network request', async () => {
    await expect(fetchAllowlistContent('http://example.com/policy.yaml')).rejects.toThrow(
      'GOOSE_ALLOWLIST must use HTTPS'
    );
    expect(mockHttpsGet).not.toHaveBeenCalled();
  });

  it('rejects a redirect to HTTP before contacting the redirect target', async () => {
    const req = mockRequest();
    mockHttpsGet.mockImplementation((_url, _options, cb) => {
      const res = mockResponse(301, { location: 'http://evil.com/policy.yaml' });
      cb(res as unknown as IncomingMessage);
      return req as unknown as ClientRequest;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'GOOSE_ALLOWLIST must use HTTPS'
    );
    expect(mockHttpsGet).toHaveBeenCalledTimes(1);
    expect(mockHttpsGet).toHaveBeenCalledWith(
      'https://example.com/policy.yaml',
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
      expect.any(Function)
    );
  });

  it('returns body content for a valid HTTPS 200 response', async () => {
    const req = mockRequest();
    mockHttpsGet.mockImplementation((_url, options, cb) => {
      expect(options.headers).toEqual({ 'accept-encoding': 'identity' });
      const res = mockResponse(200);
      cb(res as unknown as IncomingMessage);
      setTimeout(() => {
        res.emit('data', 'extensions:\n');
        res.emit('data', '  - id: x\n    command: npx x\n');
        res.emit('end');
      }, 0);
      return req as unknown as ClientRequest;
    });

    const content = await fetchAllowlistContent('https://example.com/policy.yaml');
    expect(content).toContain('extensions:');
    expect(content).toContain('npx x');
  });

  it('propagates certificate and network errors', async () => {
    const req = mockRequest();
    mockHttpsGet.mockImplementation((_url, _options, _cb) => {
      setTimeout(() => req.emit('error', new Error('certificate has expired')), 0);
      return req as unknown as ClientRequest;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'certificate has expired'
    );
  });

  it('aborts the active request after a terminal response', async () => {
    const req = mockRequest();
    const res = mockResponse(404);
    let signal: AbortSignal | undefined;
    mockHttpsGet.mockImplementation((_url, options, cb) => {
      signal = options.signal as AbortSignal;
      setTimeout(() => cb(res as unknown as IncomingMessage), 0);
      return req as unknown as ClientRequest;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'Allowlist fetch failed: HTTP 404'
    );
    expect(signal?.aborted).toBe(true);
  });

  it('follows HTTPS-to-HTTPS redirects and aborts remaining streams', async () => {
    let callCount = 0;
    let signal: AbortSignal | undefined;
    mockHttpsGet.mockImplementation((_url, options, cb) => {
      signal = options.signal as AbortSignal;
      const req = mockRequest();
      callCount++;
      if (callCount === 1) {
        const res = mockResponse(301, { location: 'https://cdn.example.com/policy.yaml' });
        cb(res as unknown as IncomingMessage);
      } else {
        const res = mockResponse(200);
        cb(res as unknown as IncomingMessage);
        setTimeout(() => {
          res.emit('data', 'extensions: []');
          res.emit('end');
        }, 0);
      }
      return req as unknown as ClientRequest;
    });

    const content = await fetchAllowlistContent('https://example.com/policy.yaml');
    expect(content).toBe('extensions: []');
    expect(signal?.aborted).toBe(true);
    expect(mockHttpsGet).toHaveBeenCalledTimes(2);
    expect(mockHttpsGet).toHaveBeenNthCalledWith(
      2,
      'https://cdn.example.com/policy.yaml',
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
      expect.any(Function)
    );
  });

  it('rejects a malformed redirect URL', async () => {
    const req = mockRequest();
    const res = mockResponse(301, { location: 'http://[' });
    mockHttpsGet.mockImplementation((_url, _options, cb) => {
      setTimeout(() => cb(res as unknown as IncomingMessage), 0);
      return req as unknown as ClientRequest;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'Invalid allowlist redirect URL'
    );
    expect(mockHttpsGet).toHaveBeenCalledTimes(1);
    expect(res.resume).toHaveBeenCalledOnce();
  });

  it('rejects after too many redirects', async () => {
    mockHttpsGet.mockImplementation((_url, _options, cb) => {
      const req = mockRequest();
      const res = mockResponse(301, { location: 'https://example.com/loop' });
      cb(res as unknown as IncomingMessage);
      return req as unknown as ClientRequest;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'Too many redirects'
    );
  });

  it('aborts when the total fetch duration expires', async () => {
    vi.useFakeTimers();
    try {
      const req = mockRequest();
      let signal: AbortSignal | undefined;
      mockHttpsGet.mockImplementation((_url, options, _cb) => {
        signal = options.signal as AbortSignal;
        return req as unknown as ClientRequest;
      });

      const rejection = expect(
        fetchAllowlistContent('https://example.com/policy.yaml')
      ).rejects.toThrow('Allowlist fetch timed out');
      await vi.advanceTimersByTimeAsync(30_000);

      await rejection;
      expect(signal?.aborted).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('rejects when https.get throws synchronously', async () => {
    mockHttpsGet.mockImplementation(() => {
      throw new Error('Invalid URI');
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'Invalid URI'
    );
  });
});

describe('parseAllowlistCommands', () => {
  it('returns commands from a valid policy', () => {
    expect(
      parseAllowlistCommands('extensions:\n  - id: example\n    command: npx example\n')
    ).toEqual(['npx example']);
  });

  it('preserves a configured empty policy as deny-all', () => {
    expect(parseAllowlistCommands('extensions: []')).toEqual([]);
  });

  it.each([
    ['', 'expected an extensions array'],
    ['name: login-page', 'expected an extensions array'],
    ['extensions: invalid', 'expected an extensions array'],
    ['extensions:\n  - id: missing-command', 'requires a command'],
    ['extensions:\n  - command: ""', 'requires a command'],
  ])('rejects an invalid policy', (content, message) => {
    expect(() => parseAllowlistCommands(content)).toThrow(message);
  });
});
