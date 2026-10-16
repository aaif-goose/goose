import fs from 'node:fs';
import path from 'node:path';
import type { Session } from 'electron';
import { describe, expect, it, vi } from 'vitest';
import {
  installBackendCertificateVerifiers,
  installScopedCertificateVerifier,
  type BackendCertificateTrustVerifier,
  type ScopedCertificateTrust,
} from './backendCertificateVerifier';

type CertificateVerifyProc = Exclude<Parameters<Session['setCertificateVerifyProc']>[0], null>;

function createMockSession() {
  let verifier: CertificateVerifyProc | null = null;
  const setCertificateVerifyProc = vi.fn((nextVerifier: CertificateVerifyProc | null) => {
    verifier = nextVerifier;
  });

  return {
    session: { setCertificateVerifyProc } as Pick<Session, 'setCertificateVerifyProc'>,
    setCertificateVerifyProc,
    verify(hostname: string, fingerprint: string): number {
      if (!verifier) {
        throw new Error('Certificate verifier was not installed');
      }

      let result: number | undefined;
      verifier(
        {
          hostname,
          certificate: { fingerprint },
        } as Parameters<CertificateVerifyProc>[0],
        (verificationResult) => {
          result = verificationResult;
        }
      );

      if (result === undefined) {
        throw new Error('Certificate verifier did not return a result');
      }
      return result;
    },
  };
}

function createTrustVerifier(initialPins: Record<string, string | null>) {
  const pins = new Map(Object.entries(initialPins));
  const verify = vi.fn((hostname: string, fingerprint: string) => {
    const pin = pins.get(hostname);
    if (pin === undefined) {
      return false;
    }
    if (pin === null) {
      pins.set(hostname, fingerprint);
      return true;
    }
    return pin === fingerprint;
  });
  const trustVerifier: BackendCertificateTrustVerifier = {
    has: (hostname) => pins.has(hostname),
    verify,
  };

  return { trustVerifier, verify };
}

function createFixture(initialPins: Record<string, string | null>) {
  const defaultSession = createMockSession();
  const rendererSession = createMockSession();
  const trust = createTrustVerifier(initialPins);
  installBackendCertificateVerifiers(
    [defaultSession.session, rendererSession.session],
    trust.trustVerifier
  );

  return { defaultSession, rendererSession, trust };
}

describe('backend certificate verifier wiring', () => {
  it('installs a scoped verifier on each window session partition', () => {
    const source = fs.readFileSync(path.resolve('src/main.ts'), 'utf8');

    expect(source).toMatch(/installScopedCertificateVerifier/);
    expect(source).toMatch(/persist:goose-/);
  });

  it.each(['defaultSession', 'rendererSession'] as const)(
    'enforces explicit pin matches and mismatches on %s',
    (sessionName) => {
      const fixture = createFixture({ 'backend.example': 'EXPLICIT-PIN' });
      const targetSession = fixture[sessionName];

      expect(targetSession.verify('backend.example', 'EXPLICIT-PIN')).toBe(0);
      expect(targetSession.verify('backend.example', 'OTHER-PIN')).toBe(-2);
    }
  );

  it.each(['defaultSession', 'rendererSession'] as const)(
    'enforces learned TOFU pin matches and mismatches on %s',
    (sessionName) => {
      const fixture = createFixture({ 'backend.example': null });
      const targetSession = fixture[sessionName];

      expect(targetSession.verify('backend.example', 'LEARNED-PIN')).toBe(0);
      expect(targetSession.verify('backend.example', 'LEARNED-PIN')).toBe(0);
      expect(targetSession.verify('backend.example', 'OTHER-PIN')).toBe(-2);
    }
  );

  it('delegates unrelated hosts to Chromium on both sessions', () => {
    const fixture = createFixture({ 'backend.example': 'EXPLICIT-PIN' });

    expect(fixture.defaultSession.verify('unrelated.example', 'ANY-PIN')).toBe(-3);
    expect(fixture.rendererSession.verify('unrelated.example', 'ANY-PIN')).toBe(-3);
    expect(fixture.trust.verify).not.toHaveBeenCalled();
  });
});

describe('installScopedCertificateVerifier', () => {
  function createScopedFixture(trust: ScopedCertificateTrust) {
    const mock = createMockSession();
    installScopedCertificateVerifier(mock.session, trust);
    return mock;
  }

  it('allows an exact fingerprint match', () => {
    const trust: ScopedCertificateTrust = { hostname: 'backend.example', fingerprint: 'AA:BB:CC' };
    const mock = createScopedFixture(trust);

    expect(mock.verify('backend.example', 'AA:BB:CC')).toBe(0);
  });

  it('rejects a fingerprint mismatch', () => {
    const trust: ScopedCertificateTrust = { hostname: 'backend.example', fingerprint: 'AA:BB:CC' };
    const mock = createScopedFixture(trust);

    expect(mock.verify('backend.example', 'DD:EE:FF')).toBe(-2);
  });

  it('TOFU-learns the fingerprint on first connection and pins it', () => {
    const trust: ScopedCertificateTrust = { hostname: 'backend.example', fingerprint: null };
    const mock = createScopedFixture(trust);

    expect(mock.verify('backend.example', 'AA:BB:CC')).toBe(0);
    expect(trust.fingerprint).toBe('AA:BB:CC');
    expect(mock.verify('backend.example', 'AA:BB:CC')).toBe(0);
    expect(mock.verify('backend.example', 'DD:EE:FF')).toBe(-2);
  });

  it('delegates unrelated hostnames to Chromium default', () => {
    const trust: ScopedCertificateTrust = { hostname: 'backend.example', fingerprint: 'AA:BB:CC' };
    const mock = createScopedFixture(trust);

    expect(mock.verify('other.example', 'AA:BB:CC')).toBe(-3);
  });

  it('does not allow one window trust to authorize another window on same hostname', () => {
    const trustA: ScopedCertificateTrust = { hostname: 'backend.example', fingerprint: 'AA:BB:CC' };
    const trustB: ScopedCertificateTrust = { hostname: 'backend.example', fingerprint: 'DD:EE:FF' };
    const sessionA = createScopedFixture(trustA);
    const sessionB = createScopedFixture(trustB);

    expect(sessionA.verify('backend.example', 'AA:BB:CC')).toBe(0);
    expect(sessionA.verify('backend.example', 'DD:EE:FF')).toBe(-2);
    expect(sessionB.verify('backend.example', 'DD:EE:FF')).toBe(0);
    expect(sessionB.verify('backend.example', 'AA:BB:CC')).toBe(-2);
  });
});
