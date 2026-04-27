import { spawn, ChildProcess } from 'child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createServer } from 'net';
import { Buffer } from 'node:buffer';
import { status } from './api';
import { Client, createClient, createConfig } from './api/client';
import {
  appendTail,
  createStartupDiagnostics,
  type StartupDiagnostics,
} from './startupDiagnostics';

export interface Logger {
  info: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
}

export const defaultLogger: Logger = {
  info: (...args) => console.log('[goosed]', ...args),
  error: (...args) => console.error('[goosed]', ...args),
};

export const findAvailablePort = (): Promise<number> => {
  return new Promise((resolve, _reject) => {
    const server = createServer();

    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address() as { port: number };
      server.close(() => {
        resolve(port);
      });
    });
  });
};

export interface FindBinaryOptions {
  isPackaged?: boolean;
  resourcesPath?: string;
}

export const findGoosedBinaryPath = (options: FindBinaryOptions = {}): string => {
  const pathFromEnv = process.env.GOOSED_BINARY;
  if (pathFromEnv) {
    if (fs.existsSync(pathFromEnv) && fs.statSync(pathFromEnv).isFile()) {
      return path.resolve(pathFromEnv);
    } else {
      throw new Error(`Invalid GOOSED_BINARY path: ${pathFromEnv} (pwd is ${process.cwd()})`);
    }
  }
  const { isPackaged = false, resourcesPath } = options;
  const binaryName = process.platform === 'win32' ? 'goosed.exe' : 'goosed';

  const possiblePaths: string[] = [];

  // Packaged app paths
  if (isPackaged && resourcesPath) {
    possiblePaths.push(path.join(resourcesPath, 'bin', binaryName));
    possiblePaths.push(path.join(resourcesPath, binaryName));
  }

  // Development paths
  possiblePaths.push(
    path.join(process.cwd(), 'src', 'bin', binaryName),
    path.join(process.cwd(), '..', '..', 'target', 'release', binaryName),
    path.join(process.cwd(), '..', '..', 'target', 'debug', binaryName)
  );

  for (const p of possiblePaths) {
    try {
      if (fs.existsSync(p) && fs.statSync(p).isFile()) {
        return p;
      }
    } catch {
      // continue
    }
  }

  throw new Error(
    `Goosed binary not found in any of the possible paths: ${possiblePaths.join(', ')}`
  );
};

export interface CheckServerStatusOptions {
  onEvent?: (name: string, details?: Record<string, unknown>) => void;
}

export const checkServerStatus = async (
  client: Client,
  errorLog: string[],
  options: CheckServerStatusOptions = {}
): Promise<boolean> => {
  const timeout = 30000;
  const interval = 100;
  const maxAttempts = Math.ceil(timeout / interval);
  options.onEvent?.('healthcheck_start', { timeoutMs: timeout, intervalMs: interval });

  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    if (errorLog.some(isFatalError)) {
      options.onEvent?.('healthcheck_fatal_error', { attempt });
      return false;
    }

    try {
      await status({ client, throwOnError: true });
      options.onEvent?.('healthcheck_success', { attempt });
      return true;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, interval));
    }
  }

  options.onEvent?.('healthcheck_timeout', { timeoutMs: timeout });
  return false;
};

export const isFatalError = (line: string): boolean => {
  const fatalPatterns = [/panicked at/, /RUST_BACKTRACE/, /fatal error/i];
  return fatalPatterns.some((pattern) => pattern.test(line));
};

export const buildGoosedEnv = (
  port: number,
  secretKey: string,
  binaryPath?: string
): Record<string, string> => {
  // Environment variable naming follows the config crate convention:
  // - GOOSE_ prefix with _ separator for top-level fields (GOOSE_PORT, GOOSE_HOST)
  // - __ separator for nested fields (GOOSE_SERVER__SECRET_KEY)
  const homeDir = process.env.HOME || os.homedir();
  const env: Record<string, string> = {
    GOOSE_PORT: port.toString(),
    GOOSE_SERVER__SECRET_KEY: secretKey,
    HOME: homeDir,
  };

  // Windows-specific environment variables
  if (process.platform === 'win32') {
    env.USERPROFILE = homeDir;
    env.APPDATA = process.env.APPDATA || path.join(homeDir, 'AppData', 'Roaming');
    env.LOCALAPPDATA = process.env.LOCALAPPDATA || path.join(homeDir, 'AppData', 'Local');
  }

  // Add binary directory to PATH for any dependencies
  const pathKey = process.platform === 'win32' ? 'Path' : 'PATH';
  const currentPath = process.env[pathKey] || '';
  if (binaryPath) {
    env[pathKey] = `${path.dirname(binaryPath)}${path.delimiter}${currentPath}`;
  } else if (currentPath) {
    env[pathKey] = currentPath;
  }

  return env;
};

// Configuration for external goosed server
export interface ExternalGoosedConfig {
  enabled: boolean;
  url?: string;
  secret?: string;
  certFingerprint?: string;
}

export interface StartGoosedOptions {
  dir?: string;
  serverSecret: string;
  env?: Record<string, string | undefined>;
  externalGoosed?: ExternalGoosedConfig;
  isPackaged?: boolean;
  resourcesPath?: string;
  logger?: Logger;
  diagnosticsDir?: string;
}

export interface GoosedResult {
  baseUrl: string;
  workingDir: string;
  process: ChildProcess | null;
  errorLog: string[];
  stopErrorLogCollection: () => void;
  cleanup: () => Promise<void>;
  client: Client;
  certFingerprint: string | null;
  startupDiagnosticsPath: string | null;
  getStartupDiagnostics: () => StartupDiagnostics | null;
  recordStartupEvent: (name: string, details?: Record<string, unknown>) => void;
}

const goosedClientForUrlAndSecret = (url: string, secret: string): Client => {
  return createClient(
    createConfig({
      baseUrl: url,
      headers: {
        'Content-Type': 'application/json',
        'X-Secret-Key': secret,
      },
    })
  );
};

export const startGoosed = async (options: StartGoosedOptions): Promise<GoosedResult> => {
  const {
    dir,
    isPackaged = false,
    resourcesPath,
    serverSecret,
    env: additionalEnv = {},
    externalGoosed,
    logger = defaultLogger,
    diagnosticsDir,
  } = options;

  const errorLog: string[] = [];
  const workingDir = dir || os.homedir();
  const startupTrace = createStartupDiagnostics(diagnosticsDir, workingDir);

  if (externalGoosed?.enabled && externalGoosed.url) {
    const url = externalGoosed.url.replace(/\/$/, '');
    logger.info(`Using external goosed backend at ${url}`);
    if (startupTrace) {
      startupTrace.diagnostics.baseUrl = url;
    }

    return {
      baseUrl: url,
      workingDir,
      process: null,
      errorLog,
      stopErrorLogCollection: () => {},
      cleanup: async () => {
        logger.info('Not killing external process that is managed externally');
      },
      client: goosedClientForUrlAndSecret(url, serverSecret),
      certFingerprint: null,
      startupDiagnosticsPath: startupTrace?.diagnosticsPath ?? null,
      getStartupDiagnostics: () => startupTrace?.diagnostics ?? null,
      recordStartupEvent: (name, details) => startupTrace?.record(name, details),
    };
  }

  if (process.env.GOOSE_EXTERNAL_BACKEND) {
    const port = process.env.GOOSE_PORT || '3000';
    const url = `https://127.0.0.1:${port}`;
    logger.info(`Using external goosed backend from env at ${url}`);
    if (startupTrace) {
      startupTrace.diagnostics.baseUrl = url;
    }

    return {
      baseUrl: url,
      workingDir,
      process: null,
      errorLog,
      stopErrorLogCollection: () => {},
      cleanup: async () => {
        logger.info('Not killing external process that is managed externally');
      },
      client: goosedClientForUrlAndSecret(url, serverSecret),
      certFingerprint: null,
      startupDiagnosticsPath: startupTrace?.diagnosticsPath ?? null,
      getStartupDiagnostics: () => startupTrace?.diagnostics ?? null,
      recordStartupEvent: (name, details) => startupTrace?.record(name, details),
    };
  }

  const goosedPath = findGoosedBinaryPath({ isPackaged, resourcesPath });

  const port = await findAvailablePort();
  logger.info(`Starting goosed from: ${goosedPath} on port ${port} in dir ${workingDir}`);

  const baseUrl = `https://127.0.0.1:${port}`;
  if (startupTrace) {
    startupTrace.diagnostics.goosedPath = goosedPath;
    startupTrace.diagnostics.baseUrl = baseUrl;
    startupTrace.record('spawn_start', { goosedPath, port, workingDir });
  }

  const spawnEnv: Record<string, string | undefined> = {
    ...process.env,
    ...buildGoosedEnv(port, serverSecret, goosedPath),
  };

  for (const [key, value] of Object.entries(additionalEnv)) {
    if (value !== undefined) {
      spawnEnv[key] = value;
    }
  }

  const spawnCommand = goosedPath;
  const spawnArgs = ['agent'];

  const isWindows = process.platform === 'win32';
  const spawnOptions = {
    env: spawnEnv,
    cwd: workingDir,
    windowsHide: true,
    detached: isWindows,
    shell: false as const,
    stdio: ['ignore', 'pipe', 'pipe'] as ['ignore', 'pipe', 'pipe'],
  };

  const safeSpawnOptions = {
    ...spawnOptions,
    env: Object.fromEntries(
      Object.entries(spawnOptions.env).map(([k, v]) =>
        k.toLowerCase().includes('secret') || k.toLowerCase().includes('key')
          ? [k, '[REDACTED]']
          : [k, v]
      )
    ),
  };
  logger.info('Spawn options:', JSON.stringify(safeSpawnOptions, null, 2));

  const goosedProcess = spawn(spawnCommand, spawnArgs, spawnOptions);
  if (startupTrace) {
    startupTrace.diagnostics.pid = goosedProcess.pid ?? null;
    startupTrace.record('spawn_success', { pid: goosedProcess.pid ?? null });
  }

  let certFingerprint: string | null = null;
  const fingerprintReady = new Promise<string | null>((resolve) => {
    const FINGERPRINT_PREFIX = 'GOOSED_CERT_FINGERPRINT=';
    let resolved = false;

    goosedProcess.stdout?.on('data', (data: Buffer) => {
      const text = data.toString();
      logger.info(`goosed stdout for port ${port} and dir ${workingDir}: ${text}`);

      if (!resolved && text.includes(FINGERPRINT_PREFIX)) {
        for (const line of text.split('\n')) {
          if (line.startsWith(FINGERPRINT_PREFIX)) {
            certFingerprint = line.slice(FINGERPRINT_PREFIX.length).trim();
            logger.info(`Pinned cert fingerprint: ${certFingerprint}`);
            if (startupTrace) {
              startupTrace.diagnostics.certFingerprintSeen = true;
              startupTrace.record('fingerprint_received', { certFingerprint });
            }
            resolved = true;
            resolve(certFingerprint);
            break;
          }
        }
      }
    });

    goosedProcess.on('exit', () => {
      if (!resolved) {
        resolved = true;
        resolve(null);
      }
    });
  });

  // Once we have the fingerprint (or the process exits before emitting one),
  // remove the stdout listener. Leaving it attached for the lifetime of the
  // long-running goosed process means every chunk of stdout data triggers
  // Node's internal EmitToJSStreamListener::OnStreamRead which converts raw
  // bytes into a JS string via v8::String::NewFromTwoByte. Over multi-hour
  // sessions this has been observed to hit a V8 assertion and crash the
  // Electron main process. Removing the listener and calling resume()
  // lets the pipe drain harmlessly without buffering into Node/V8.
  void fingerprintReady.then(() => {
    goosedProcess.stdout?.removeAllListeners('data');
    goosedProcess.stdout?.resume();
  });

  const onStderrData = (data: Buffer) => {
    const lines = data.toString().split('\n');
    const nonEmptyLines = lines.filter((line) => line.trim());
    appendTail(startupTrace?.diagnostics.stderrTail ?? [], nonEmptyLines);
    for (const line of lines) {
      if (line.trim()) {
        errorLog.push(line);
        if (isFatalError(line)) {
          logger.error(`goosed stderr for port ${port} and dir ${workingDir}: ${line}`);
        }
      }
    }
  };
  goosedProcess.stderr?.on('data', onStderrData);

  const stopErrorLogCollection = () => {
    goosedProcess.stderr?.off('data', onStderrData);
  };

  goosedProcess.on('exit', (code, signal) => {
    logger.info(`goosed process exited with code ${code} for port ${port} and dir ${workingDir}`);
    if (startupTrace) {
      startupTrace.diagnostics.childExitCode = code;
      startupTrace.diagnostics.childExitSignal = signal;
      startupTrace.record('child_exit', { code, signal });
    }
  });

  goosedProcess.on('error', (err) => {
    logger.error(`Failed to start goosed on port ${port} and dir ${workingDir}`, err);
    errorLog.push(err.message);
    startupTrace?.record('spawn_error', { message: err.message, name: err.name });
  });

  const cleanup = async (): Promise<void> => {
    return new Promise<void>((resolve) => {
      if (!goosedProcess || goosedProcess.killed) {
        resolve();
        return;
      }

      goosedProcess.on('close', () => {
        resolve();
      });

      logger.info('Terminating goosed server');
      try {
        if (process.platform === 'win32') {
          spawn('taskkill', ['/pid', goosedProcess.pid!.toString(), '/f', '/t']);
        } else {
          goosedProcess.kill('SIGTERM');
        }
      } catch (error) {
        logger.error('Error while terminating goosed process:', error);
      }

      setTimeout(() => {
        if (goosedProcess && !goosedProcess.killed && process.platform !== 'win32') {
          goosedProcess.kill('SIGKILL');
        }
        resolve();
      }, 5000);
    });
  };

  logger.info(`Goosed server successfully started on port ${port}`);

  await fingerprintReady;

  return {
    baseUrl,
    workingDir,
    process: goosedProcess,
    errorLog,
    stopErrorLogCollection,
    cleanup,
    client: goosedClientForUrlAndSecret(baseUrl, serverSecret),
    certFingerprint,
    startupDiagnosticsPath: startupTrace?.diagnosticsPath ?? null,
    getStartupDiagnostics: () => startupTrace?.diagnostics ?? null,
    recordStartupEvent: (name, details) => startupTrace?.record(name, details),
  };
};
