import { describe, expect, it } from "vitest";
import { filterModelProvidersForDistro } from "./distroProviderConstraints";

describe("filterModelProvidersForDistro", () => {
  const providers = [
    {
      id: "anthropic",
      displayName: "Anthropic",
      category: "model",
      description: "Claude models",
      setupMethod: "single_api_key",
      group: "default",
    },
    {
      id: "openai",
      displayName: "OpenAI",
      category: "model",
      description: "GPT models",
      setupMethod: "single_api_key",
      group: "default",
    },
    {
      id: "ollama",
      displayName: "Ollama",
      category: "model",
      description: "Local models",
      setupMethod: "local",
      group: "default",
    },
  ] as const;

  it("returns all providers when no distro is present", () => {
    expect(
      filterModelProvidersForDistro([...providers], { present: false }),
    ).toEqual(providers);
  });

  it("returns all providers when no allowlist is configured", () => {
    expect(
      filterModelProvidersForDistro([...providers], {
        present: true,
      }),
    ).toEqual(providers);
  });

  it("filters providers to the configured allowlist", () => {
    expect(
      filterModelProvidersForDistro([...providers], {
        present: true,
        providerAllowlist: "openai, ollama",
      }),
    ).toEqual([providers[1], providers[2]]);
  });

  it("ignores whitespace and empty allowlist items", () => {
    expect(
      filterModelProvidersForDistro([...providers], {
        present: true,
        providerAllowlist: "  anthropic ,, openai  ",
      }),
    ).toEqual([providers[0], providers[1]]);
  });
});
