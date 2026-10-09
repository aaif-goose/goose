import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import log from './logger';

/** Append `dir` so tools the user installed win; leave PATH alone if `dir` is already on it. */
export function appendToWindowsPath(currentPath: string, dir: string): string {
  const entries = currentPath.split(';').filter(Boolean);
  if (entries.some((entry) => entry.toLowerCase() === dir.toLowerCase())) {
    return currentPath;
  }
  return [...entries, dir].join(';');
}

/**
 * Ensures Windows shims are available in %LOCALAPPDATA%\Goose\bin
 * This allows the bundled executables to be found via PATH regardless of where Goose is installed
 */
export async function ensureWinShims(): Promise<void> {
  if (process.platform !== 'win32') return;

  const srcDir = path.join(process.resourcesPath, 'bin'); // existing dir
  const tgtDir = path.join(
    process.env.LOCALAPPDATA ?? path.join(os.homedir(), 'AppData', 'Local'),
    'Goose',
    'bin'
  );

  try {
    await fs.promises.mkdir(tgtDir, { recursive: true });

    // Copy command-line tools only; the goose binary is never shimmed
    const shims = ['uvx.exe', 'uv.exe', 'npx.cmd'];

    await Promise.all(
      shims.map(async (shim) => {
        const src = path.join(srcDir, shim);
        const dst = path.join(tgtDir, shim);
        try {
          // Check if source file exists before attempting to copy
          await fs.promises.access(src);
          await fs.promises.copyFile(src, dst); // overwrites with newer build
          log.info(`Copied Windows shim: ${shim} to ${dst}`);
        } catch (e) {
          log.error(`Failed to copy shim ${shim}`, e);
        }
      })
    );

    // For this process and its children only; the user's permanent PATH is untouched.
    process.env.PATH = appendToWindowsPath(process.env.PATH ?? '', tgtDir);
  } catch (error) {
    log.error('Failed to ensure Windows shims:', error);
  }
}
