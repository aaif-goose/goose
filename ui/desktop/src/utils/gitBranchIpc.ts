import { execFile } from 'child_process';
import { ipcMain } from 'electron';

const gitArgs = (dir: string, args: string[]) => [
  '-c',
  'safe.bareRepository=explicit',
  '-c',
  'core.fsmonitor=false',
  '-C',
  dir,
  ...args,
];

const git = (dir: string, args: string[], timeout = 3000) =>
  new Promise<string>((resolve, reject) => {
    execFile('git', gitArgs(dir, args), { timeout }, (error, stdout) => {
      if (error) reject(error);
      else resolve(stdout.trim());
    });
  });

const getCurrentBranch = async (dir: string) => {
  try {
    const ref = await git(dir, ['symbolic-ref', 'HEAD']);
    return ref.startsWith('refs/heads/') ? ref.slice('refs/heads/'.length) : ref;
  } catch {
    return git(dir, ['rev-parse', '--short', 'HEAD']).catch(() => null);
  }
};

ipcMain.handle(
  'get-git-branch-info',
  async (_event, dir: string): Promise<{ branch: string } | null> => {
    if (!dir?.trim()) return null;

    const branch = await getCurrentBranch(dir);
    return branch ? { branch } : null;
  }
);

ipcMain.handle('list-git-branches', async (_event, dir: string): Promise<string[]> => {
  if (!dir?.trim()) return [];

  try {
    const branches = await git(dir, [
      'for-each-ref',
      'refs/heads/',
      '--format=%(refname:lstrip=2)',
    ]);
    return branches.split('\n').filter(Boolean);
  } catch {
    return [];
  }
});

ipcMain.handle(
  'switch-git-branch',
  async (_event, dir: string, branch: string): Promise<{ success: boolean; error?: string }> => {
    if (!dir?.trim() || !branch?.trim()) return { success: false };

    try {
      await git(dir, ['checkout', branch], 30000);
      return { success: true };
    } catch (error) {
      const currentBranch = await getCurrentBranch(dir);
      if (currentBranch === branch) return { success: true };

      const gitError = error as Error & { stderr?: string };
      return { success: false, error: gitError.stderr?.toString() || gitError.message };
    }
  }
);
