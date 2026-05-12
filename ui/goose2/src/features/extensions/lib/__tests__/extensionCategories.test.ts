import { describe, expect, it } from "vitest";
import type { ExtensionEntry } from "../../types";
import {
  classifyExtension,
  filterExtensions,
  getExtensionCategoryCounts,
  splitExtensionsByCategory,
} from "../extensionCategories";

function extension(
  name: string,
  type: ExtensionEntry["type"],
  description = "",
): ExtensionEntry {
  return {
    type,
    name,
    description,
    config_key: name,
    enabled: true,
    ...(type === "stdio" ? { cmd: "npx", args: [] } : {}),
    ...(type === "streamable_http" ? { uri: "http://localhost:3000/mcp" } : {}),
  } as ExtensionEntry;
}

const labelForCategory = (category: string) =>
  category === "gooseCapabilities" ? "Goose capabilities" : "Apps & services";

describe("extension categories", () => {
  it("classifies built-in and platform extensions as Goose capabilities", () => {
    expect(classifyExtension(extension("developer", "builtin"))).toBe(
      "gooseCapabilities",
    );
    expect(classifyExtension(extension("computer", "platform"))).toBe(
      "gooseCapabilities",
    );
    expect(classifyExtension(extension("github", "stdio"))).toBe(
      "appsServices",
    );
  });

  it("filters by search text across name, description, and category label", () => {
    const extensions = [
      extension("github", "stdio", "Issue tracker"),
      extension("developer", "builtin", "Code tools"),
    ];

    expect(
      filterExtensions({
        extensions,
        searchTerm: "issue",
        activeFilter: "all",
        getCategoryLabel: labelForCategory,
      }).map((item) => item.name),
    ).toEqual(["github"]);

    expect(
      filterExtensions({
        extensions,
        searchTerm: "goose",
        activeFilter: "all",
        getCategoryLabel: labelForCategory,
      }).map((item) => item.name),
    ).toEqual(["developer"]);
  });

  it("counts and splits extensions by category", () => {
    const extensions = [
      extension("developer", "builtin"),
      extension("computer", "platform"),
      extension("github", "stdio"),
    ];

    expect(getExtensionCategoryCounts(extensions)).toEqual({
      appsServices: 1,
      gooseCapabilities: 2,
    });
    expect(splitExtensionsByCategory(extensions)).toMatchObject({
      primaryExtensions: [{ name: "github" }],
      gooseCapabilities: [{ name: "developer" }, { name: "computer" }],
    });
  });
});
