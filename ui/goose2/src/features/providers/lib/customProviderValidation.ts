import { parseCustomProviderModels } from "./customProviderModels";
import { validateCustomProviderHeaders } from "./customProviderHeaders";
import {
  isCustomProviderEngine,
  normalizeCustomProviderEngine,
} from "./customProviderDraft";
import type { CustomProviderDraft } from "./customProviderTypes";

export type CustomProviderValidationField =
  | "displayName"
  | "engine"
  | "apiUrl"
  | "apiKey"
  | "models"
  | "headers";

export interface CustomProviderValidationIssue {
  field: CustomProviderValidationField;
  key:
    | "settings.providers.custom.validation.displayNameRequired"
    | "settings.providers.custom.validation.engineRequired"
    | "settings.providers.custom.validation.apiUrlRequired"
    | "settings.providers.custom.validation.apiUrlInvalid"
    | "settings.providers.custom.validation.apiKeyRequired"
    | "settings.providers.custom.validation.modelsRequired"
    | "settings.providers.custom.validation.headerNameRequired"
    | "settings.providers.custom.validation.headerValueRequired"
    | "settings.providers.custom.validation.headerNameInvalid"
    | "settings.providers.custom.validation.headerDuplicate";
  message: string;
  index?: number;
}

export interface CustomProviderValidationOptions {
  requireApiKey?: boolean;
}

export class CustomProviderValidationError extends Error {
  readonly issues: CustomProviderValidationIssue[];

  constructor(issues: CustomProviderValidationIssue[]) {
    super("Custom provider validation failed.");
    this.name = "CustomProviderValidationError";
    this.issues = issues;
  }
}

function isProbablyUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export function validateCustomProviderDraft(
  draft: CustomProviderDraft,
  options: CustomProviderValidationOptions = {},
): CustomProviderValidationIssue[] {
  const issues: CustomProviderValidationIssue[] = [];
  const apiUrl = draft.apiUrl.trim();
  const models = parseCustomProviderModels(
    draft.models.length > 0 ? draft.models : draft.modelsInput,
  );
  const requireApiKey =
    options.requireApiKey ?? (draft.requiresAuth && !draft.apiKeySet);
  const engine = normalizeCustomProviderEngine(draft.engine);

  if (!draft.displayName.trim()) {
    issues.push({
      field: "displayName",
      key: "settings.providers.custom.validation.displayNameRequired",
      message: "Display name is required.",
    });
  }

  if (!isCustomProviderEngine(engine)) {
    issues.push({
      field: "engine",
      key: "settings.providers.custom.validation.engineRequired",
      message: "Choose a provider engine.",
    });
  }

  if (!apiUrl) {
    issues.push({
      field: "apiUrl",
      key: "settings.providers.custom.validation.apiUrlRequired",
      message: "API URL is required.",
    });
  } else if (!isProbablyUrl(apiUrl)) {
    issues.push({
      field: "apiUrl",
      key: "settings.providers.custom.validation.apiUrlInvalid",
      message: "Enter a valid HTTP or HTTPS URL.",
    });
  }

  if (requireApiKey && !draft.apiKey.trim()) {
    issues.push({
      field: "apiKey",
      key: "settings.providers.custom.validation.apiKeyRequired",
      message: "API key is required.",
    });
  }

  if (models.length === 0) {
    issues.push({
      field: "models",
      key: "settings.providers.custom.validation.modelsRequired",
      message: "Add at least one model.",
    });
  }

  issues.push(...validateCustomProviderHeaders(draft.headers));

  return issues;
}

export function assertValidCustomProviderDraft(
  draft: CustomProviderDraft,
  options?: CustomProviderValidationOptions,
): void {
  const issues = validateCustomProviderDraft(draft, options);
  if (issues.length > 0) {
    throw new CustomProviderValidationError(issues);
  }
}
