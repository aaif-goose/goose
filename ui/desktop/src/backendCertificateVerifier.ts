import { Buffer } from 'node:buffer';
import type { Session } from 'electron';

export interface BackendCertificateTrustVerifier {
  has(hostname: string): boolean;
  verify(hostname: string, fingerprint: string): boolean;
}

export interface ScopedCertificateTrust {
  hostname: string;
  fingerprint: string | null;
}

type CertificateVerifierSession = Pick<Session, 'setCertificateVerifyProc'>;

export function normalizeFingerprint(fp: string): string {
  if (fp.startsWith('sha256/')) {
    const b64 = fp.slice('sha256/'.length);
    const buf = Buffer.from(b64, 'base64');
    return Array.from(buf)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join(':')
      .toUpperCase();
  }
  return fp.toUpperCase();
}

export function installBackendCertificateVerifiers(
  targetSessions: CertificateVerifierSession[],
  trustVerifier: BackendCertificateTrustVerifier
): void {
  for (const targetSession of targetSessions) {
    targetSession.setCertificateVerifyProc((request, callback) => {
      if (!trustVerifier.has(request.hostname)) {
        callback(-3);
        return;
      }

      const match = trustVerifier.verify(request.hostname, request.certificate.fingerprint);
      callback(match ? 0 : -2);
    });
  }
}

export function installScopedCertificateVerifier(
  targetSession: CertificateVerifierSession,
  trust: ScopedCertificateTrust
): void {
  targetSession.setCertificateVerifyProc((request, callback) => {
    if (request.hostname.toLowerCase() !== trust.hostname) {
      callback(-3);
      return;
    }
    const fp = normalizeFingerprint(request.certificate.fingerprint);
    if (trust.fingerprint === null) {
      trust.fingerprint = fp;
      callback(0);
      return;
    }
    callback(trust.fingerprint === fp ? 0 : -2);
  });
}
