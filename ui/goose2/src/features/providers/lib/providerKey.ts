export function normalizeProviderKey(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "_")
    .split("_")
    .filter(Boolean)
    .join("_");
}
