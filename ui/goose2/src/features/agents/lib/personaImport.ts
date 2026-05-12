const JSON_MIME_TYPES = new Set([
  "",
  "application/json",
  "application/x-json",
  "text/json",
  "text/plain",
]);

export interface ImportMessageDescriptor {
  key:
    | "view.importInvalidExtension"
    | "view.importInvalidMimeType"
    | "view.imported_one"
    | "view.imported_other";
  options?: Record<string, unknown>;
}

export function validatePersonaImportFile(
  file: Pick<File, "name" | "type">,
): ImportMessageDescriptor | null {
  const lowerName = file.name.toLowerCase();
  if (!lowerName.endsWith(".json")) {
    return {
      key: "view.importInvalidExtension",
    } satisfies ImportMessageDescriptor;
  }

  if (!JSON_MIME_TYPES.has(file.type)) {
    return {
      key: "view.importInvalidMimeType",
    } satisfies ImportMessageDescriptor;
  }

  return null;
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
  if (typeof error === "string" && error.trim().length > 0) {
    return error;
  }
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message;
  }
  return fallback;
}
