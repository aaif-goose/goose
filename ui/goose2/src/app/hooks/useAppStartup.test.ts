import { describe, expect, it } from "vitest";
import { filterStartupProvidersForDistro } from "./useAppStartup";

const providers = [
  { id: "goose", label: "Goose" },
  { id: "codex-acp", label: "Codex" },
];

describe("filterStartupProvidersForDistro", () => {
  it("keeps providers when no allowlist is configured", () => {
    expect(filterStartupProvidersForDistro(providers, null, [], true)).toEqual(
      providers,
    );
  });

  it("keeps Goose while the catalog is still loading", () => {
    expect(
      filterStartupProvidersForDistro(
        providers,
        new Set(["anthropic"]),
        [],
        false,
      ),
    ).toEqual(providers);
  });

  it("keeps Goose after catalog load when an allowed model provider exists", () => {
    expect(
      filterStartupProvidersForDistro(
        providers,
        new Set(["anthropic"]),
        [{ id: "anthropic" }],
        true,
      ),
    ).toEqual(providers);
  });

  it("removes Goose after catalog load when no model provider is allowed", () => {
    expect(
      filterStartupProvidersForDistro(
        providers,
        new Set(["anthropic"]),
        [{ id: "openai" }],
        true,
      ),
    ).toEqual([{ id: "codex-acp", label: "Codex" }]);
  });
});
