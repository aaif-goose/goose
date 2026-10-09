import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import log from './logger';

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

    // Append to PATH for this process and its children only, so tools the user
    // installed win. This does not modify the user's permanent PATH.
    const currentPath = process.env.PATH ?? '';
    if (!currentPath.toLowerCase().includes(tgtDir.toLowerCase())) {
      process.env.PATH = `${currentPath}${path.delimiter}${tgtDir}`;
      log.info(`Added ${tgtDir} to PATH for Goose processes only`);
    }
  } catch (error) {
    log.error('Failed to ensure Windows shims:', error);
  }
}
