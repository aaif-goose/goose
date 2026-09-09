import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { createGooseServeStartupDiagnostics } from './startupDiagnostics';

const tempDirs: string[] = [];

function makeTempDir(): string {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'startup-diagnostics-test-'));
  tempDirs.push(tempDir);
  return tempDir;
}

describe('startup diagnostics', () => {
  afterEach(() => {
    while (tempDirs.length > 0) {
      const tempDir = tempDirs.pop();
      if (tempDir) {
        fs.rmSync(tempDir, { recursive: true, force: true });
      }
    }
  });

  it('writes serve startup diagnostics with serve-specific fields', () => {
    const diagnosticsDir = makeTempDir();
    const trace = createGooseServeStartupDiagnostics(diagnosticsDir, '/tmp/project');

    expect(trace).not.toBeNull();
    trace!.diagnostics.binaryPath = '/bin/goose';
    trace!.diagnostics.httpBaseUrl = 'http://127.0.0.1:3000';
    trace!.diagnostics.readinessUrl = 'http://127.0.0.1:3000/status';
    trace!.diagnostics.statusUrl = 'http://127.0.0.1:3000/status';
    trace!.diagnostics.healthUrl = 'http://127.0.0.1:3000/health';
    trace!.diagnostics.acpUrl = 'ws://127.0.0.1:3000/acp?token=REDACTED';
    trace!.record('healthcheck_start', {
      transport: 'plain-http',
      method: 'GET',
      path: '/status',
    });
    trace!.record('healthcheck_success', { attempt: 1 });

    expect(path.basename(trace!.diagnosticsPath)).toMatch(/^goose-serve-startup-.*\.json$/);
    const saved = JSON.parse(fs.readFileSync(trace!.diagnosticsPath, 'utf8'));
    expect(saved).toMatchObject({
      binaryPath: '/bin/goose',
      httpBaseUrl: 'http://127.0.0.1:3000',
      readinessUrl: 'http://127.0.0.1:3000/status',
      statusUrl: 'http://127.0.0.1:3000/status',
      healthUrl: 'http://127.0.0.1:3000/health',
      acpUrl: 'ws://127.0.0.1:3000/acp?token=REDACTED',
      healthCheckSucceeded: true,
    });
    expect(saved).not.toHaveProperty('goosedPath');
    expect(saved).not.toHaveProperty('certFingerprintSeen');
    expect(saved.events.map((event: { name: string }) => event.name)).toEqual([
      'healthcheck_start',
      'healthcheck_success',
    ]);
  });
});
