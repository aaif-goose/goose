import { describe, it, expect, vi, beforeEach } from 'vitest';
import { EventEmitter } from 'events';
import type { IncomingMessage, ClientRequest } from 'node:http';

type GetFn = (url: string | URL, cb: (res: IncomingMessage) => void) => ClientRequest;

const { mockHttpsGet } = vi.hoisted(() => ({ mockHttpsGet: vi.fn<GetFn>() }));

vi.mock('node:https', () => ({
  default: { get: mockHttpsGet },
}));

import { fetchAllowlistContent } from './allowlistFetcher';

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
    mockHttpsGet.mockImplementation((_url, cb) => {
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
      expect.any(Function)
    );
  });

  it('returns body content for a valid HTTPS 200 response', async () => {
    const req = mockRequest();
    mockHttpsGet.mockImplementation((_url, cb) => {
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
    mockHttpsGet.mockImplementation((_url, _cb) => {
      setTimeout(() => req.emit('error', new Error('certificate has expired')), 0);
      return req as unknown as ClientRequest;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'certificate has expired'
    );
  });

  it('follows HTTPS-to-HTTPS redirects and returns body', async () => {
    let callCount = 0;
    mockHttpsGet.mockImplementation((_url, cb) => {
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
    expect(mockHttpsGet).toHaveBeenCalledTimes(2);
    expect(mockHttpsGet).toHaveBeenNthCalledWith(
      2,
      'https://cdn.example.com/policy.yaml',
      expect.any(Function)
    );
  });

  it('rejects after too many redirects', async () => {
    mockHttpsGet.mockImplementation((_url, cb) => {
      const req = mockRequest();
      const res = mockResponse(301, { location: 'https://example.com/loop' });
      cb(res as unknown as IncomingMessage);
      return req as unknown as ClientRequest;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'Too many redirects'
    );
  });
});
