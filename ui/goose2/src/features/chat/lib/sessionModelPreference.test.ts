import { beforeEach, describe, expect, it } from "vitest";
import { resolveSessionModelPreference } from "./sessionModelPreference";

describe("resolveSessionModelPreference", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("keeps a requested concrete provider when the stored preference uses a different provider", () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "claude-sonnet-4",
          modelName: "Claude Sonnet 4",
          providerId: "anthropic",
        },
      }),
    );

    expect(
      resolveSessionModelPreference({
        providerId: "openai",
      }),
    ).toEqual({
      providerId: "openai",
    });
  });

  it("reuses a stored model when it matches the requested concrete provider", () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "gpt-5.4",
          modelName: "GPT-5.4",
          providerId: "openai",
        },
      }),
    );

    expect(
      resolveSessionModelPreference({
        providerId: "openai",
      }),
    ).toEqual({
      providerId: "openai",
      modelId: "gpt-5.4",
      modelName: "GPT-5.4",
    });
  });

  it("resolves an agent provider to the stored concrete provider and model", () => {
    window.localStorage.setItem(
      "goose:preferredModelsByAgent",
      JSON.stringify({
        goose: {
          modelId: "claude-sonnet-4",
          modelName: "Claude Sonnet 4",
          providerId: "anthropic",
        },
      }),
    );

    expect(
      resolveSessionModelPreference({
        providerId: "goose",
      }),
    ).toEqual({
      providerId: "anthropic",
      modelId: "claude-sonnet-4",
      modelName: "Claude Sonnet 4",
    });
  });
});
