import { describe, it, expect } from 'vitest';
import { buildConnectSrc, shouldUpgradeInsecureRequests, buildCSP } from '../csp';
import type { ExternalGoosedConfig } from '../settings';

describe('buildConnectSrc', () => {
  it('includes default sources when no external backend is configured', () => {
    const result = buildConnectSrc(undefined);
    expect(result).toContain("'self'");
    expect(result).toContain('http://127.0.0.1:*');
    expect(result).toContain('wss://127.0.0.1:*');
  });

  it('includes external backend origin when enabled', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'http://dev.company.net:12604',
      secret: 'test',
    };
    const result = buildConnectSrc(config);
    expect(result).toContain('http://dev.company.net:12604');
    expect(result).toContain('ws://dev.company.net:12604');
  });

  it('includes external secure WebSocket origin for HTTPS backends', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'https://secure.company.net:12604',
      secret: 'test',
    };
    const result = buildConnectSrc(config);
    expect(result).toContain('https://secure.company.net:12604');
    expect(result).toContain('wss://secure.company.net:12604');
  });

  it('does not include external origin when disabled', () => {
    const config: ExternalGoosedConfig = {
      enabled: false,
      url: 'http://dev.company.net:12604',
      secret: 'test',
    };
    const result = buildConnectSrc(config);
    expect(result).not.toContain('dev.company.net');
  });

  it('handles invalid URLs gracefully', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'not-a-valid-url',
      secret: 'test',
    };
    const result = buildConnectSrc(config);
    expect(result).toContain("'self'");
    expect(result).not.toContain('not-a-valid-url');
  });
});

describe('shouldUpgradeInsecureRequests', () => {
  it('returns true when no external backend is configured', () => {
    expect(shouldUpgradeInsecureRequests(undefined)).toBe(true);
  });

  it('returns true when external backend is disabled', () => {
    const config: ExternalGoosedConfig = {
      enabled: false,
      url: 'http://dev.company.net:12604',
      secret: 'test',
    };
    expect(shouldUpgradeInsecureRequests(config)).toBe(true);
  });

  it('returns false when external backend uses HTTP', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'http://dev.company.net:12604',
      secret: 'test',
    };
    expect(shouldUpgradeInsecureRequests(config)).toBe(false);
  });

  it('returns true when external backend uses HTTPS', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'https://dev.company.net:12604',
      secret: 'test',
    };
    expect(shouldUpgradeInsecureRequests(config)).toBe(true);
  });

  it('returns true for invalid URLs', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'not-a-url',
      secret: 'test',
    };
    expect(shouldUpgradeInsecureRequests(config)).toBe(true);
  });

  it('returns true when URL is empty', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: '',
      secret: 'test',
    };
    expect(shouldUpgradeInsecureRequests(config)).toBe(true);
  });
});

describe('buildCSP', () => {
  it('includes upgrade-insecure-requests with no external backend', () => {
    const csp = buildCSP(undefined);
    expect(csp).toContain('upgrade-insecure-requests');
  });

  it('includes upgrade-insecure-requests with HTTPS external backend', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'https://secure.company.net:12604',
      secret: 'test',
    };
    const csp = buildCSP(config);
    expect(csp).toContain('upgrade-insecure-requests');
    expect(csp).toContain('https://secure.company.net:12604');
  });

  it('excludes upgrade-insecure-requests with HTTP external backend', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'http://dev.company.net:12604',
      secret: 'test',
    };
    const csp = buildCSP(config);
    expect(csp).not.toContain('upgrade-insecure-requests');
    expect(csp).toContain('http://dev.company.net:12604');
  });

  it('always includes core directives', () => {
    const config: ExternalGoosedConfig = {
      enabled: true,
      url: 'http://dev.company.net:12604',
      secret: 'test',
    };
    const csp = buildCSP(config);
    expect(csp).toContain("default-src 'self'");
    expect(csp).toContain("script-src 'self' 'unsafe-inline'");
    expect(csp).toContain('connect-src');
    expect(csp).toContain("object-src 'none'");
  });
});
