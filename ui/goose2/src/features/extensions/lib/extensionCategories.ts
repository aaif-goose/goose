import type { ExtensionEntry } from "../types";
import { getDisplayName } from "../types";

export type ExtensionCategory = "appsServices" | "gooseCapabilities";

export type ExtensionFilter = "all" | ExtensionCategory;

export const EXTENSION_CATEGORIES: readonly ExtensionCategory[] = [
  "appsServices",
  "gooseCapabilities",
];

const GOOSE_CAPABILITY_TYPES = new Set(["builtin", "platform"]);
export function classifyExtension(
  extension: ExtensionEntry,
): ExtensionCategory {
  if (GOOSE_CAPABILITY_TYPES.has(extension.type)) {
    return "gooseCapabilities";
  }
  return "appsServices";
}

export function compareExtensionsByName(
  a: ExtensionEntry,
  b: ExtensionEntry,
): number {
  return getDisplayName(a).localeCompare(getDisplayName(b));
}

export function getExtensionCategoryCounts(
  extensions: ExtensionEntry[],
): Record<ExtensionCategory, number> {
  const counts: Record<ExtensionCategory, number> = {
    appsServices: 0,
    gooseCapabilities: 0,
  };
  for (const extension of extensions) {
    counts[classifyExtension(extension)] += 1;
  }
  return counts;
}

export function filterExtensions(options: {
  extensions: ExtensionEntry[];
  searchTerm: string;
  activeFilter: ExtensionFilter;
  getCategoryLabel: (category: ExtensionCategory) => string;
}): ExtensionEntry[] {
  const { extensions, searchTerm, activeFilter, getCategoryLabel } = options;
  const query = searchTerm.toLowerCase();

  return extensions
    .filter((extension) => {
      const category = classifyExtension(extension);
      const matchesSearch =
        !query ||
        getDisplayName(extension).toLowerCase().includes(query) ||
        extension.name.toLowerCase().includes(query) ||
        (extension.description ?? "").toLowerCase().includes(query) ||
        getCategoryLabel(category).toLowerCase().includes(query);

      return (
        matchesSearch && (activeFilter === "all" || category === activeFilter)
      );
    })
    .sort(compareExtensionsByName);
}

export function splitExtensionsByCategory(extensions: ExtensionEntry[]): {
  primaryExtensions: ExtensionEntry[];
  gooseCapabilities: ExtensionEntry[];
} {
  const primaryExtensions: ExtensionEntry[] = [];
  const gooseCapabilities: ExtensionEntry[] = [];

  for (const extension of extensions) {
    if (classifyExtension(extension) === "gooseCapabilities") {
      gooseCapabilities.push(extension);
    } else {
      primaryExtensions.push(extension);
    }
  }

  return { primaryExtensions, gooseCapabilities };
}
