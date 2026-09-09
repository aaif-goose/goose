import fs from 'node:fs';
import path from 'node:path';

export interface StartupTraceEvent {
  name: string;
  at: string;
  elapsedMs: number;
  details?: Record<string, unknown>;
}

export interface GooseServeStartupDiagnostics {
  attemptId: string;
  startedAt: string;
  binaryPath: string | null;
  workingDir: string;
  httpBaseUrl: string | null;
  readinessUrl: string | null;
  statusUrl: string | null;
  healthUrl: string | null;
  acpUrl: string | null;
  pid: number | null;
  healthCheckSucceeded: boolean;
  childExitCode: number | null;
  childExitSignal: string | null;
  stderrTail: string[];
  events: StartupTraceEvent[];
}

export interface GooseServeStartupTrace {
  diagnosticsPath: string;
  diagnostics: GooseServeStartupDiagnostics;
  record: (name: string, details?: Record<string, unknown>) => void;
  flush: () => void;
}

const STARTUP_TAIL_LIMIT = 80;
const STARTUP_LOGS_TO_KEEP = 20;

export const appendTail = (target: string[], lines: string[]) => {
  target.push(...lines.filter((line) => line.trim()));
  if (target.length > STARTUP_TAIL_LIMIT) {
    target.splice(0, target.length - STARTUP_TAIL_LIMIT);
  }
};

const cleanupGooseServeStartupDiagnostics = (diagnosticsDir: string) => {
  const startupLogs = fs
    .readdirSync(diagnosticsDir, { withFileTypes: true })
    .filter(
      (entry) =>
        entry.isFile() &&
        entry.name.startsWith('goose-serve-startup-') &&
        entry.name.endsWith('.json')
    )
    .map((entry) => {
      const filePath = path.join(diagnosticsDir, entry.name);
      return {
        filePath,
        modifiedMs: fs.statSync(filePath).mtimeMs,
      };
    })
    .sort((a, b) => b.modifiedMs - a.modifiedMs);

  for (const startupLog of startupLogs.slice(STARTUP_LOGS_TO_KEEP)) {
    fs.unlinkSync(startupLog.filePath);
  }
};

export const createGooseServeStartupDiagnostics = (
  diagnosticsDir: string | undefined,
  workingDir: string
): GooseServeStartupTrace | null => {
  if (!diagnosticsDir) {
    return null;
  }

  fs.mkdirSync(diagnosticsDir, { recursive: true });
  cleanupGooseServeStartupDiagnostics(diagnosticsDir);
  const startedAt = new Date();
  const attemptId = `goose-serve-startup-${startedAt.toISOString().replace(/:/g, '-')}-${process.pid}.json`;
  const diagnosticsPath = path.join(diagnosticsDir, attemptId);
  const monotonicStart = Date.now();

  const diagnostics: GooseServeStartupDiagnostics = {
    attemptId,
    startedAt: startedAt.toISOString(),
    binaryPath: null,
    workingDir,
    httpBaseUrl: null,
    readinessUrl: null,
    statusUrl: null,
    healthUrl: null,
    acpUrl: null,
    pid: null,
    healthCheckSucceeded: false,
    childExitCode: null,
    childExitSignal: null,
    stderrTail: [],
    events: [],
  };

  const flush = () => {
    fs.writeFileSync(diagnosticsPath, `${JSON.stringify(diagnostics, null, 2)}\n`);
  };

  const record = (name: string, details?: Record<string, unknown>) => {
    if (name === 'healthcheck_success') {
      diagnostics.healthCheckSucceeded = true;
    }
    diagnostics.events.push({
      name,
      at: new Date().toISOString(),
      elapsedMs: Date.now() - monotonicStart,
      ...(details ? { details } : {}),
    });
    flush();
  };

  flush();

  return {
    diagnosticsPath,
    diagnostics,
    record,
    flush,
  };
};
