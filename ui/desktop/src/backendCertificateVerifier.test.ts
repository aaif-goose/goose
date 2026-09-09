import fs from 'node:fs';
import path from 'node:path';
import type { Session } from 'electron';
import { describe, expect, it, vi } from 'vitest';
import {
  installBackendCertificateVerifiers,
  type BackendCertificateTrustVerifier,
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
  it('covers the renderer session that opens ACP WebSockets', () => {
    const source = fs.readFileSync(path.resolve('src/main.ts'), 'utf8');

    expect(source).toMatch(
      /installBackendCertificateVerifiers\([\s\S]{0,200}session\.defaultSession[\s\S]{0,200}session\.fromPartition\('persist:goose'\)/
    );
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
