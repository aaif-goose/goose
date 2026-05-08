export function formatSkillError(error: unknown, fallback: string): string {
  const message =
    typeof error === "string" && error.trim().length > 0
      ? error.trim()
      : error instanceof Error && error.message.trim().length > 0
        ? error.message.trim()
        : fallback;

  if (typeof error !== "object" || error === null || !("data" in error)) {
    return message;
  }

  return `${message}\n${JSON.stringify(error.data)}`;
}
