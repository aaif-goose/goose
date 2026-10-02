import { describe, it, expect, vi, beforeEach } from 'vitest';
import { EventEmitter } from 'events';

vi.mock('node:https', () => ({
  default: { get: vi.fn() },
}));

import https from 'node:https';
import { fetchAllowlistContent } from './allowlistFetcher';

function mockResponse(statusCode: number, headers: Record<string, string> = {}) {
  const res = new EventEmitter() as any;
  res.statusCode = statusCode;
  res.headers = headers;
  res.setEncoding = vi.fn();
  res.resume = vi.fn();
  return res;
}

function mockRequest() {
  return new EventEmitter() as any;
}

describe('fetchAllowlistContent', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('rejects an HTTP initial URL without making a network request', async () => {
    await expect(fetchAllowlistContent('http://example.com/policy.yaml')).rejects.toThrow(
      'GOOSE_ALLOWLIST must use HTTPS'
    );
    expect(https.get).not.toHaveBeenCalled();
  });

  it('rejects a redirect to HTTP before contacting the redirect target', async () => {
    const req = mockRequest();
    (https.get as any).mockImplementation((_url: string, cb: (res: any) => void) => {
      const res = mockResponse(301, { location: 'http://evil.com/policy.yaml' });
      cb(res);
      return req;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'GOOSE_ALLOWLIST must use HTTPS'
    );
    expect(https.get).toHaveBeenCalledTimes(1);
    expect(https.get).toHaveBeenCalledWith('https://example.com/policy.yaml', expect.any(Function));
  });

  it('returns body content for a valid HTTPS 200 response', async () => {
    const req = mockRequest();
    (https.get as any).mockImplementation((_url: string, cb: (res: any) => void) => {
      const res = mockResponse(200);
      cb(res);
      setImmediate(() => {
        res.emit('data', 'extensions:\n');
        res.emit('data', '  - id: x\n    command: npx x\n');
        res.emit('end');
      });
      return req;
    });

    const content = await fetchAllowlistContent('https://example.com/policy.yaml');
    expect(content).toContain('extensions:');
    expect(content).toContain('npx x');
  });

  it('propagates certificate and network errors', async () => {
    const req = mockRequest();
    (https.get as any).mockImplementation((_url: string, _cb: (res: any) => void) => {
      setImmediate(() => req.emit('error', new Error('certificate has expired')));
      return req;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'certificate has expired'
    );
  });

  it('follows HTTPS-to-HTTPS redirects and returns body', async () => {
    let callCount = 0;
    (https.get as any).mockImplementation((url: string, cb: (res: any) => void) => {
      const req = mockRequest();
      callCount++;
      if (callCount === 1) {
        const res = mockResponse(301, { location: 'https://cdn.example.com/policy.yaml' });
        cb(res);
      } else {
        const res = mockResponse(200);
        cb(res);
        setImmediate(() => {
          res.emit('data', 'extensions: []');
          res.emit('end');
        });
      }
      return req;
    });

    const content = await fetchAllowlistContent('https://example.com/policy.yaml');
    expect(content).toBe('extensions: []');
    expect(https.get).toHaveBeenCalledTimes(2);
    expect(https.get).toHaveBeenNthCalledWith(
      2,
      'https://cdn.example.com/policy.yaml',
      expect.any(Function)
    );
  });

  it('rejects after too many redirects', async () => {
    (https.get as any).mockImplementation((_url: string, cb: (res: any) => void) => {
      const req = mockRequest();
      const res = mockResponse(301, { location: 'https://example.com/loop' });
      cb(res);
      return req;
    });

    await expect(fetchAllowlistContent('https://example.com/policy.yaml')).rejects.toThrow(
      'Too many redirects'
    );
  });
});
