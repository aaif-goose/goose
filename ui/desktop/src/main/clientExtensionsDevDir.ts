import fs from 'node:fs';
import path from 'node:path';

export function findDevClientExtensionsDir(isPackaged: boolean): string | undefined {
  if (isPackaged) {
    return undefined;
  }

  const candidates = [
    path.join(process.cwd(), 'examples', 'client-extensions'),
    path.join(process.cwd(), '..', '..', 'examples', 'client-extensions'),
  ];

  return candidates.find(
    (candidate) => fs.existsSync(candidate) && fs.statSync(candidate).isDirectory()
  );
}
