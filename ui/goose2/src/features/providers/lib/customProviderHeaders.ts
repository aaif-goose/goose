import type { CustomProviderHeaderDraft } from "./customProviderTypes";

export interface CustomProviderHeaderIssue {
  field: "headers";
  key:
    | "settings.providers.custom.validation.headerNameRequired"
    | "settings.providers.custom.validation.headerValueRequired"
    | "settings.providers.custom.validation.headerNameInvalid"
    | "settings.providers.custom.validation.headerDuplicate";
  message: string;
  index?: number;
}

const HEADER_TOKEN_RE = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
let nextHeaderId = 0;

export function createCustomProviderHeaderDraft(
  key = "",
  value = "",
): CustomProviderHeaderDraft {
  nextHeaderId += 1;
  return {
    id: `header-${nextHeaderId}`,
    key,
    value,
  };
}

export function normalizeHeaderName(name: string): string {
  return name.trim();
}

export function normalizeHeaderValue(value: string): string {
  return value.trim();
}

export function recordToHeaderDrafts(
  headers?: Record<string, string> | null,
): CustomProviderHeaderDraft[] {
  return Object.entries(headers ?? {}).map(([key, value], index) => ({
    id: `server-header-${index}`,
    key,
    value,
  }));
}

export function headerDraftsToRecord(
  headers: CustomProviderHeaderDraft[],
): Record<string, string> | undefined {
  const record: Record<string, string> = {};

  for (const header of headers) {
    const key = normalizeHeaderName(header.key);
    const value = normalizeHeaderValue(header.value);
    if (key && value) {
      record[key] = value;
    }
  }

  return Object.keys(record).length > 0 ? record : undefined;
}

export function validateCustomProviderHeaders(
  headers: CustomProviderHeaderDraft[],
): CustomProviderHeaderIssue[] {
  const issues: CustomProviderHeaderIssue[] = [];
  const seen = new Map<string, number>();

  headers.forEach((header, index) => {
    const key = normalizeHeaderName(header.key);
    const value = normalizeHeaderValue(header.value);
    const normalizedKey = key.toLowerCase();

    if (!key && !value) {
      return;
    }

    if (!key) {
      issues.push({
        field: "headers",
        key: "settings.providers.custom.validation.headerNameRequired",
        message: "Header name is required.",
        index,
      });
      return;
    }

    if (!HEADER_TOKEN_RE.test(key)) {
      issues.push({
        field: "headers",
        key: "settings.providers.custom.validation.headerNameInvalid",
        message: "Header names can only contain valid HTTP token characters.",
        index,
      });
    }

    if (!value) {
      issues.push({
        field: "headers",
        key: "settings.providers.custom.validation.headerValueRequired",
        message: "Header value is required.",
        index,
      });
    }

    if (seen.has(normalizedKey)) {
      issues.push({
        field: "headers",
        key: "settings.providers.custom.validation.headerDuplicate",
        message: "Header names must be unique.",
        index,
      });
      return;
    }

    seen.set(normalizedKey, index);
  });

  return issues;
}
