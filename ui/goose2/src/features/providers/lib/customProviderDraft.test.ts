import { describe, expect, it } from "vitest";
import {
  createEmptyCustomProviderDraft,
  customProviderDraftToUpsertRequest,
  readToCustomProviderDraft,
  templateToCustomProviderDraft,
} from "./customProviderDraft";
import {
  headerDraftsToRecord,
  recordToHeaderDrafts,
  validateCustomProviderHeaders,
} from "./customProviderHeaders";
import {
  formatCustomProviderModels,
  parseCustomProviderModels,
} from "./customProviderModels";
import { validateCustomProviderDraft } from "./customProviderValidation";

describe("custom provider helper functions", () => {
  it("parses model input from comma and newline separated values", () => {
    expect(
      parseCustomProviderModels("claude-3-5-sonnet, gpt-4.1\n gpt-4.1"),
    ).toEqual(["claude-3-5-sonnet", "gpt-4.1"]);
    expect(formatCustomProviderModels(["a", "a", "b"])).toBe("a, b");
  });

  it("converts header records to drafts and ignores blank draft rows on submit", () => {
    expect(recordToHeaderDrafts({ Authorization: "Bearer token" })).toEqual([
      {
        id: "server-header-0",
        key: "Authorization",
        value: "Bearer token",
      },
    ]);
    expect(
      headerDraftsToRecord([
        { id: "a", key: " X-Test ", value: " enabled " },
        { id: "b", key: "", value: "" },
      ]),
    ).toEqual({
      "X-Test": "enabled",
    });
  });

  it("reports header validation issues with stable i18n keys", () => {
    const issues = validateCustomProviderHeaders([
      { id: "a", key: "Bad Header", value: "value" },
      { id: "b", key: "X-Test", value: "" },
      { id: "c", key: "x-test", value: "duplicate" },
    ]);

    expect(issues.map((issue) => issue.key)).toEqual([
      "settings.providers.custom.validation.headerNameInvalid",
      "settings.providers.custom.validation.headerValueRequired",
      "settings.providers.custom.validation.headerDuplicate",
    ]);
  });

  it("builds a draft from a catalog template", () => {
    const draft = templateToCustomProviderDraft({
      providerId: "acme",
      name: "Acme AI",
      format: "openai",
      apiUrl: "https://api.acme.test/v1",
      models: [
        {
          id: "acme-large",
          name: "Acme Large",
          contextLimit: 128000,
          capabilities: {
            toolCall: true,
            reasoning: false,
            attachment: false,
            temperature: true,
          },
          deprecated: false,
        },
        {
          id: "acme-old",
          name: "Acme Old",
          contextLimit: 8192,
          capabilities: {
            toolCall: false,
            reasoning: false,
            attachment: false,
            temperature: true,
          },
          deprecated: true,
        },
      ],
      supportsStreaming: true,
      envVar: "ACME_API_KEY",
      docUrl: "https://acme.test/docs",
    });

    expect(draft).toMatchObject({
      engine: "openai_compatible",
      displayName: "Acme AI",
      apiUrl: "https://api.acme.test/v1",
      models: ["acme-large"],
      modelsInput: "acme-large",
      catalogProviderId: "acme",
    });
  });

  it("builds a draft from an editable read response", () => {
    const draft = readToCustomProviderDraft({
      provider: {
        providerId: "acme_ai",
        engine: "openai_compatible",
        displayName: "Acme AI",
        apiUrl: "https://api.acme.test/v1",
        models: ["acme-large"],
        supportsStreaming: true,
        headers: { "X-Test": "enabled" },
        requiresAuth: true,
        catalogProviderId: "acme",
        basePath: "/v1",
        apiKeyEnv: "ACME_AI_API_KEY",
        apiKeySet: true,
      },
      editable: true,
      status: { providerId: "acme_ai", isConfigured: true },
    });

    expect(draft).toMatchObject({
      providerId: "acme_ai",
      headers: [{ id: "server-header-0", key: "X-Test", value: "enabled" }],
      basePath: "/v1",
    });
  });

  it("validates required fields and maps draft fields to ACP upsert input", () => {
    const emptyIssues = validateCustomProviderDraft(
      createEmptyCustomProviderDraft(),
    );
    expect(emptyIssues.map((issue) => issue.key)).toContain(
      "settings.providers.custom.validation.displayNameRequired",
    );

    const draft = {
      ...createEmptyCustomProviderDraft(),
      displayName: " Acme AI ",
      apiUrl: " https://api.acme.test/v1 ",
      apiKey: " secret ",
      modelsInput: "acme-large, acme-small",
      headers: [{ id: "a", key: " X-Test ", value: " enabled " }],
      basePath: " /v1 ",
      catalogProviderId: "acme",
    };

    expect(validateCustomProviderDraft(draft)).toEqual([]);
    expect(customProviderDraftToUpsertRequest(draft)).toEqual({
      engine: "openai_compatible",
      displayName: "Acme AI",
      apiUrl: "https://api.acme.test/v1",
      apiKey: "secret",
      models: ["acme-large", "acme-small"],
      supportsStreaming: true,
      headers: {
        "X-Test": "enabled",
      },
      requiresAuth: true,
      catalogProviderId: "acme",
      basePath: "/v1",
    });
  });

  it("omits unchanged API keys and preserves stable header ids", () => {
    const draft = {
      ...createEmptyCustomProviderDraft(),
      providerId: "acme_ai",
      displayName: "Acme AI",
      apiUrl: "https://api.acme.test/v1",
      apiKeySet: true,
      models: ["acme-large"],
      headers: [
        { id: "stable", key: "X-Original", value: "enabled" },
        { id: "empty", key: "", value: "" },
      ],
    };

    expect(validateCustomProviderDraft(draft)).toEqual([]);
    expect(customProviderDraftToUpsertRequest(draft)).not.toHaveProperty(
      "apiKey",
    );

    const nextHeaders = draft.headers.map((header) =>
      header.id === "stable" ? { ...header, key: "X-Renamed" } : header,
    );
    expect(nextHeaders[0].id).toBe("stable");
  });

  it("surfaces unknown engines as invalid instead of normalizing them", () => {
    const draft = {
      ...createEmptyCustomProviderDraft(),
      engine: "future_engine",
      displayName: "Future AI",
      apiUrl: "https://api.future.test/v1",
      apiKey: "secret",
      models: ["future-large"],
    };

    expect(
      validateCustomProviderDraft(draft).map((issue) => issue.field),
    ).toContain("engine");
  });
});
