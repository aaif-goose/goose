export interface ImportMessageDescriptor {
  key: "view.imported_one" | "view.imported_other";
  options?: Record<string, unknown>;
}

export function formatImportSuccessMessage(
  importedCount: number,
): ImportMessageDescriptor {
  if (importedCount === 1) {
    return { key: "view.imported_one", options: { count: importedCount } };
  }

  return {
    key: "view.imported_other",
    options: { count: importedCount },
  };
}

export function formatAgentError(error: unknown, fallback: string): string {
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
