import type { FixedExtensionEntry } from '../../ConfigContext';

function isBundledExtension(extension: FixedExtensionEntry): boolean {
  return 'bundled' in extension && extension.bundled === true;
}

function getSourcePriority(extension: FixedExtensionEntry): number {
  if (extension.type === 'builtin') return 0;
  if (isBundledExtension(extension)) return 1;
  return 2;
}

export function sortExtensionsBySourcePriority(
  extensions: FixedExtensionEntry[]
): FixedExtensionEntry[] {
  return [...extensions].sort((a, b) => {
    const priorityDelta = getSourcePriority(a) - getSourcePriority(b);
    if (priorityDelta !== 0) return priorityDelta;

    return a.name.localeCompare(b.name);
  });
}
