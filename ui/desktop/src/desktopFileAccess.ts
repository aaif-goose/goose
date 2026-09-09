import fs from 'node:fs/promises';
import { constants as fsConstants } from 'node:fs';
import type { Stats } from 'node:fs';
import path from 'node:path';

export interface FileReadResult {
  file: string;
  filePath: string;
  error: string | null;
  found: boolean;
}

interface FileAccessRequestProvenance {
  isRegisteredWindow: boolean;
  isMainFrame: boolean;
  rendererUrl: string;
}

export function isAppRendererUrl(rendererUrl: string, expectedUrl: URL): boolean {
  try {
    const actual = new URL(rendererUrl);
    if (expectedUrl.protocol === 'file:') {
      return (
        actual.protocol === 'file:' &&
        actual.host === expectedUrl.host &&
        actual.pathname === expectedUrl.pathname
      );
    }
    return actual.origin === expectedUrl.origin && actual.pathname === expectedUrl.pathname;
  } catch {
    return false;
  }
}

export function isAuthorizedFileAccessRequest(
  provenance: FileAccessRequestProvenance,
  expectedUrl: URL
): boolean {
  return (
    provenance.isRegisteredWindow &&
    provenance.isMainFrame &&
    isAppRendererUrl(provenance.rendererUrl, expectedUrl)
  );
}

function isMissingFile(error: unknown): boolean {
  return (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    (error as { code?: unknown }).code === 'ENOENT'
  );
}

function missingFile(filePath: string): FileReadResult {
  return { file: '', filePath, error: null, found: false };
}

function failedRead(filePath: string, message: string): FileReadResult {
  return { file: '', filePath, error: message, found: false };
}

type WorkingDirectoryBinding =
  | { status: 'ready'; path: string; dev: bigint; ino: bigint }
  | { status: 'missing'; path: string }
  | { status: 'error'; path: string };

export class DesktopFileAccess {
  private readonly workingDirectories = new Map<number, WorkingDirectoryBinding>();

  private bindingForWindow(windowId: number): WorkingDirectoryBinding {
    const binding = this.workingDirectories.get(windowId);
    if (!binding) {
      throw new Error('This window is not authorized to access .goosehints');
    }
    return binding;
  }

  private async bindingMatchesDirectory(binding: WorkingDirectoryBinding): Promise<boolean> {
    if (binding.status !== 'ready') {
      return false;
    }
    try {
      const metadata = await fs.lstat(binding.path, { bigint: true });
      return (
        metadata.isDirectory() &&
        !metadata.isSymbolicLink() &&
        metadata.dev === binding.dev &&
        metadata.ino === binding.ino
      );
    } catch {
      return false;
    }
  }

  async bindWindow(windowId: number, workingDirectory: string): Promise<void> {
    const resolvedPath = path.resolve(workingDirectory);
    try {
      const canonicalPath = await fs.realpath(resolvedPath);
      const metadata = await fs.lstat(canonicalPath, { bigint: true });
      if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
        throw new Error('Working directory is not a regular directory');
      }
      this.workingDirectories.set(windowId, {
        status: 'ready',
        path: canonicalPath,
        dev: metadata.dev,
        ino: metadata.ino,
      });
    } catch (error) {
      this.workingDirectories.set(windowId, {
        status: isMissingFile(error) ? 'missing' : 'error',
        path: resolvedPath,
      });
    }
  }

  unbindWindow(windowId: number): void {
    this.workingDirectories.delete(windowId);
  }

  async readGoosehints(windowId: number): Promise<FileReadResult> {
    const binding = this.bindingForWindow(windowId);
    const filePath = path.join(binding.path, '.goosehints');
    if (binding.status === 'missing') {
      return missingFile(filePath);
    }
    if (binding.status === 'error') {
      return failedRead(filePath, 'Unable to resolve the working directory');
    }
    if (!(await this.bindingMatchesDirectory(binding))) {
      return failedRead(filePath, 'The working directory changed after it was authorized');
    }

    try {
      const metadata = await fs.lstat(filePath);
      if (metadata.isSymbolicLink()) {
        return failedRead(filePath, 'Refusing to read a symbolic link as .goosehints');
      }
      if (!metadata.isFile()) {
        return failedRead(filePath, '.goosehints is not a regular file');
      }

      const canonicalFilePath = await fs.realpath(filePath);
      if (path.dirname(canonicalFilePath) !== binding.path) {
        return failedRead(filePath, '.goosehints resolves outside the working directory');
      }

      const noFollow = process.platform === 'win32' ? 0 : fsConstants.O_NOFOLLOW;
      const handle = await fs.open(canonicalFilePath, fsConstants.O_RDONLY | noFollow);
      try {
        const openedMetadata = await handle.stat();
        if (!openedMetadata.isFile()) {
          return failedRead(filePath, '.goosehints is not a regular file');
        }
        if (openedMetadata.dev !== metadata.dev || openedMetadata.ino !== metadata.ino) {
          return failedRead(filePath, '.goosehints changed while it was being opened');
        }
        if (!(await this.bindingMatchesDirectory(binding))) {
          return failedRead(filePath, 'The working directory changed after it was authorized');
        }
        return {
          file: await handle.readFile('utf8'),
          filePath,
          error: null,
          found: true,
        };
      } finally {
        await handle.close();
      }
    } catch (error) {
      if (isMissingFile(error)) {
        return missingFile(filePath);
      }
      return failedRead(filePath, 'Unable to read .goosehints');
    }
  }

  async writeGoosehints(windowId: number, content: string): Promise<boolean> {
    const binding = this.bindingForWindow(windowId);
    if (binding.status !== 'ready' || typeof content !== 'string') {
      return false;
    }
    if (!(await this.bindingMatchesDirectory(binding))) {
      return false;
    }

    const filePath = path.join(binding.path, '.goosehints');
    const noFollow = process.platform === 'win32' ? 0 : fsConstants.O_NOFOLLOW;
    try {
      let metadata: Stats;
      try {
        metadata = await fs.lstat(filePath);
      } catch (error) {
        if (!isMissingFile(error)) {
          return false;
        }
        if (!(await this.bindingMatchesDirectory(binding))) {
          return false;
        }

        const handle = await fs.open(
          filePath,
          fsConstants.O_WRONLY | fsConstants.O_CREAT | fsConstants.O_EXCL | noFollow,
          0o666
        );
        try {
          if (!(await handle.stat()).isFile()) {
            return false;
          }
          if (!(await this.bindingMatchesDirectory(binding))) {
            return false;
          }
          await handle.writeFile(content, 'utf8');
          return true;
        } finally {
          await handle.close();
        }
      }

      if (metadata.isSymbolicLink() || !metadata.isFile()) {
        return false;
      }

      const handle = await fs.open(filePath, fsConstants.O_WRONLY | noFollow);
      try {
        const openedMetadata = await handle.stat();
        if (
          !openedMetadata.isFile() ||
          openedMetadata.dev !== metadata.dev ||
          openedMetadata.ino !== metadata.ino
        ) {
          return false;
        }
        if (!(await this.bindingMatchesDirectory(binding))) {
          return false;
        }
        await handle.truncate(0);
        await handle.writeFile(content, 'utf8');
        return true;
      } finally {
        await handle.close();
      }
    } catch {
      return false;
    }
  }
}

export async function readSelectedRecipe(filePath: string): Promise<FileReadResult> {
  const extension = path.extname(filePath).toLowerCase();
  if (extension !== '.yaml' && extension !== '.yml') {
    return failedRead(filePath, 'The selected recipe must be a YAML file');
  }

  try {
    const nonBlocking = process.platform === 'win32' ? 0 : fsConstants.O_NONBLOCK;
    const handle = await fs.open(filePath, fsConstants.O_RDONLY | nonBlocking);
    try {
      const metadata = await handle.stat();
      if (!metadata.isFile()) {
        return failedRead(filePath, 'The selected recipe is not a regular file');
      }
      return {
        file: await handle.readFile('utf8'),
        filePath,
        error: null,
        found: true,
      };
    } finally {
      await handle.close();
    }
  } catch (error) {
    if (isMissingFile(error)) {
      return missingFile(filePath);
    }
    return failedRead(filePath, 'Unable to read the selected recipe');
  }
}
