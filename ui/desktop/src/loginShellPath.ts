import { spawn } from 'child_process';

import type { Logger } from './gooseServe';

const RESOLVE_TIMEOUT_MS = 5000;

/**
 * Resolve the user's PATH by running their interactive login shell.
 *
 * Apps launched from Finder, the Dock or a Linux app menu get the session's
 * PATH, which misses what shell profiles add (Homebrew, nvm, pyenv, ...).
 * Resolving it here rather than in goose keeps the goose CLI on the PATH it
 * was started with. Inside Flatpak this would run the sandbox's shell, so goose
 * resolves the host's PATH itself there.
 */
const resolveLoginShellPath = (logger?: Logger): Promise<string | null> => {
  if (process.platform === 'win32' || process.env.FLATPAK_ID) {
    return Promise.resolve(null);
  }

  const shell = process.env.SHELL || 'bash';

  return new Promise((resolve) => {
    // detached: a new session keeps the interactive shell's job-control setup
    // from stealing the terminal foreground and suspending the app.
    // Use `printenv PATH` instead of `echo $PATH` so the command is
    // shell-neutral: fish treats $PATH as a list and space-joins it under
    // `echo`, which would corrupt the resolved PATH for fish users.
    const child = spawn(shell, ['-l', '-i', '-c', 'printenv PATH'], {
      stdio: ['ignore', 'pipe', 'ignore'],
      detached: true,
      windowsHide: true,
    });

    const timer = setTimeout(() => {
      child.kill();
      resolve(null);
    }, RESOLVE_TIMEOUT_MS);
    timer.unref?.();

    let stdout = '';
    child.stdout?.on('data', (chunk: Buffer) => {
      stdout += chunk.toString('utf8');
    });
    child.on('error', (error) => {
      clearTimeout(timer);
      logger?.error('Failed to resolve login shell PATH', error);
      resolve(null);
    });
    child.on('close', (code) => {
      clearTimeout(timer);
      const path = stdout.trim().split('\n').pop()?.trim();
      resolve(code === 0 && path ? path : null);
    });
  });
};

let cached: Promise<string | null> | undefined;

/** Resolve the login-shell PATH once per app run, caching the result. */
export const getLoginShellPath = (logger?: Logger): Promise<string | null> => {
  cached ??= resolveLoginShellPath(logger);
  return cached;
};
