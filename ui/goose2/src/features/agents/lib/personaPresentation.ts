export function getPersonaInitials(displayName: string): string {
  const initials = displayName
    .trim()
    .split(/\s+/)
    .map((part) => part.match(/[\p{L}\p{N}]/u)?.[0] ?? "")
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase();

  return initials || "?";
}
