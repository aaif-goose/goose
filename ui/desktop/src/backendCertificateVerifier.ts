import type { Session } from 'electron';

export interface BackendCertificateTrustVerifier {
  has(hostname: string): boolean;
  verify(hostname: string, fingerprint: string): boolean;
}

type CertificateVerifierSession = Pick<Session, 'setCertificateVerifyProc'>;

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
