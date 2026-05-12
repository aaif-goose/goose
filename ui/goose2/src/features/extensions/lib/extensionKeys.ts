export function nameToKey(name: string): string {
  return name
    .replace(/\s/g, "")
    .replace(/[^a-zA-Z0-9_-]/g, "_")
    .toLowerCase();
}

export const normalizeExtensionKey = nameToKey;
