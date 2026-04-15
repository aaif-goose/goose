/**
 * mesh-llm process lifecycle — download, start, stop, auto-start.
 *
 * macOS (Apple Silicon) only for download/spawn; other platforms can still
 * probe the API port to detect an externally-running mesh.
 */

import { execFile, execFileSync, spawn } from 'child_process';
import path from 'node:path';
import os from 'node:os';
import fsSync from 'node:fs';
import http from 'node:http';
import { Buffer } from 'node:buffer';
import log from './utils/logger';
import { Client } from './api/client';
import { readConfig } from './api/sdk.gen';

const API_PORT = 9337;
const CONSOLE_PORT = 3131;
const DOWNLOAD_URL =
  'https://github.com/michaelneale/mesh-llm/releases/latest/download/mesh-bundle.tar.gz';

let childProcess: ReturnType<typeof spawn> | null = null;

function execFileP(cmd: string, args: string[], opts: { timeout: number }): Promise<void> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, opts, (err) => (err ? reject(err) : resolve()));
  });
}

// ── Binary discovery ────────────────────────────────────────────────

export async function findBinary(): Promise<string | null> {
  try {
    const binPath = execFileSync('which', ['mesh-llm'], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim();
    if (binPath) return binPath;
  } catch {
    // ignore — which returns non-zero if not found
  }

  const meshDir = path.join(os.homedir(), '.mesh-llm', 'mesh-llm');
  if (fsSync.existsSync(meshDir)) return meshDir;

  const localBin = path.join(os.homedir(), '.local', 'bin', 'mesh-llm');
  if (fsSync.existsSync(localBin)) return localBin;

  return null;
}

export async function downloadBinary(): Promise<{ binary: string } | { error: string }> {
  if (process.platform !== 'darwin' || process.arch !== 'arm64') {
    return { error: 'Auto-download is only available on macOS (Apple Silicon)' };
  }

  const installDir = path.join(os.homedir(), '.mesh-llm');
  if (!fsSync.existsSync(installDir)) {
    fsSync.mkdirSync(installDir, { recursive: true });
  }

  const tarball = path.join(installDir, 'mesh-bundle.tar.gz');
  try {
    await execFileP('curl', ['-fsSL', '-o', tarball, DOWNLOAD_URL], { timeout: 120000 });
    await execFileP('tar', ['xz', '--strip-components=1', '-C', installDir, '-f', tarball], {
      timeout: 30000,
    });

    const binary = path.join(installDir, 'mesh-llm');
    if (!fsSync.existsSync(binary)) {
      return { error: 'Download succeeded but mesh-llm binary not found' };
    }

    for (const name of ['mesh-llm', 'rpc-server', 'llama-server']) {
      const bin = path.join(installDir, name);
      if (fsSync.existsSync(bin)) {
        try {
          await execFileP('codesign', ['-s', '-', bin], { timeout: 10000 });
        } catch {
          /* codesign may fail if already signed */
        }
        try {
          await execFileP('xattr', ['-cr', bin], { timeout: 10000 });
        } catch {
          /* xattr may fail */
        }
      }
    }

    return { binary };
  } catch (err) {
    return { error: `Download failed: ${err}` };
  } finally {
    try {
      fsSync.unlinkSync(tarball);
    } catch {
      /* ignore */
    }
  }
}

// ── Port probing ────────────────────────────────────────────────────

export function isRunning(): Promise<boolean> {
  return new Promise((resolve) => {
    const req = http.get(`http://localhost:${API_PORT}/v1/models`, { timeout: 2000 }, (res) => {
      res.resume();
      resolve(res.statusCode === 200);
    });
    req.on('error', () => resolve(false));
    req.on('timeout', () => {
      req.destroy();
      resolve(false);
    });
  });
}

// ── Status check (used by check-mesh IPC) ───────────────────────────

export interface MeshStatus {
  running: boolean;
  installed: boolean;
  models: string[];
  token?: string;
  peerCount?: number;
  nodeStatus?: string;
  binaryPath?: string;
}

export async function check(): Promise<MeshStatus> {
  const result: MeshStatus = { running: false, installed: true, models: [] };

  const binary = await findBinary();
  if (binary) {
    result.binaryPath = binary;
  } else {
    result.installed = false;
  }

  // Probe the API
  try {
    const modelsData = await new Promise<{ running: boolean; models: string[] }>((resolve) => {
      const req = http.get(`http://localhost:${API_PORT}/v1/models`, { timeout: 3000 }, (res) => {
        let body = '';
        res.on('data', (chunk: Buffer) => {
          body += chunk.toString();
        });
        res.on('end', () => {
          try {
            if (res.statusCode !== 200) {
              resolve({ running: false, models: [] });
              return;
            }
            const data = JSON.parse(body);
            if (!Array.isArray(data.data)) {
              resolve({ running: false, models: [] });
              return;
            }
            const models = data.data
              .filter((m: { id?: unknown }) => typeof m.id === 'string')
              .map((m: { id: string }) => m.id);
            resolve({ running: true, models });
          } catch {
            resolve({ running: false, models: [] });
          }
        });
      });
      req.on('error', () => resolve({ running: false, models: [] }));
      req.on('timeout', () => {
        req.destroy();
        resolve({ running: false, models: [] });
      });
    });

    result.running = modelsData.running;
    result.models = modelsData.models;
  } catch {
    // API not reachable
  }

  if (result.running) {
    try {
      const statusData = await new Promise<{
        token?: string;
        peerCount?: number;
        nodeStatus?: string;
      }>((resolve) => {
        const req = http.get(
          `http://localhost:${CONSOLE_PORT}/api/status`,
          { timeout: 3000 },
          (res) => {
            let body = '';
            res.on('data', (chunk: Buffer) => {
              body += chunk.toString();
            });
            res.on('end', () => {
              try {
                const data = JSON.parse(body);
                resolve({
                  token: data.token,
                  peerCount: Array.isArray(data.peers) ? data.peers.length : undefined,
                  nodeStatus: data.node_status,
                });
              } catch {
                resolve({});
              }
            });
          }
        );
        req.on('error', () => resolve({}));
        req.on('timeout', () => {
          req.destroy();
          resolve({});
        });
      });
      result.token = statusData.token;
      result.peerCount = statusData.peerCount;
      result.nodeStatus = statusData.nodeStatus;
    } catch {
      // console not available
    }
  }

  return result;
}

// ── Start / stop ────────────────────────────────────────────────────

function spawnAttached(
  binary: string,
  args: string[]
): Promise<{ started: boolean; error?: string; pid?: number }> {
  const logDir = path.join(os.homedir(), '.mesh-llm');
  if (!fsSync.existsSync(logDir)) fsSync.mkdirSync(logDir, { recursive: true });
  const logPath = path.join(logDir, 'mesh-llm.log');
  const out = fsSync.openSync(logPath, 'a');

  const child = spawn(binary, args, { stdio: ['ignore', out, out] });
  childProcess = child;
  child.on('exit', () => {
    if (childProcess === child) childProcess = null;
  });

  return new Promise((resolve) => {
    const timeout = setTimeout(() => {
      child.removeAllListeners('error');
      resolve({ started: true, pid: child.pid });
    }, 500);

    child.once('error', (err) => {
      clearTimeout(timeout);
      childProcess = null;
      resolve({ started: false, error: `Failed to spawn mesh-llm: ${err.message}` });
    });
  }).then((result) => {
    fsSync.closeSync(out);
    return result as { started: boolean; error?: string; pid?: number };
  });
}

export async function start(
  args: string[]
): Promise<{ started: boolean; error?: string; pid?: number; alreadyRunning?: boolean }> {
  if (await isRunning()) {
    return { started: true, alreadyRunning: true };
  }

  const dlResult = await downloadBinary();
  let binary: string;
  if ('error' in dlResult) {
    const existing = await findBinary();
    if (!existing) {
      return { started: false, error: dlResult.error };
    }
    binary = existing;
  } else {
    binary = dlResult.binary;
  }

  return spawnAttached(binary, args);
}

export async function stop(): Promise<{ stopped: boolean }> {
  if (childProcess) {
    cleanup();
    return { stopped: true };
  }
  try {
    const binary = await findBinary();
    if (!binary) return { stopped: false };
    execFileSync(binary, ['stop'], { timeout: 5000, encoding: 'utf8' });
    return { stopped: true };
  } catch {
    return { stopped: false };
  }
}

export function cleanup(): void {
  if (!childProcess) return;
  try {
    childProcess.kill('SIGTERM');
  } catch {
    /* already dead */
  }
  childProcess = null;
}

// ── Startup check ───────────────────────────────────────────────────

export async function checkProviderRunning(goosedClient: Client): Promise<boolean> {
  const res = await readConfig({
    body: { key: 'GOOSE_PROVIDER', is_secret: false },
    client: goosedClient,
  });
  const provider = typeof res.data === 'string' ? res.data : String(res.data ?? '');
  if (!provider.includes('mesh')) return true;
  if (await isRunning()) return true;

  log.info('Mesh provider configured but not running');
  return false;
}
