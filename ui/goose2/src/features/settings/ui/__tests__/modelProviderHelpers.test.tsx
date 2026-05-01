import { describe, expect, it } from "vitest";
import { getFieldSetupDescription } from "../modelProviderHelpers";

const t = (key: string) => key;

describe("getFieldSetupDescription", () => {
  it("uses single API key copy for config fields with one required secret API key", () => {
    expect(
      getFieldSetupDescription("config_fields", t, [
        {
          key: "OPENAI_API_KEY",
          label: "API Key",
          secret: true,
          required: true,
          placeholder: "Paste your API key",
        },
      ]),
    ).toBe("providers.models.setup.fieldDescription.singleApiKey");
  });

  it("uses generic config fields copy for config fields without an API key field", () => {
    expect(
      getFieldSetupDescription("config_fields", t, [
        {
          key: "OLLAMA_HOST",
          label: "Host",
          secret: false,
          required: true,
          placeholder: "localhost or http://localhost:11434",
        },
      ]),
    ).toBe("providers.models.setup.fieldDescription.configFields");
  });
});
