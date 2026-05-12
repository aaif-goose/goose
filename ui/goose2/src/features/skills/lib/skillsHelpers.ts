import type { SkillInfo } from "../api/skills";

export type SkillsFilter = "all" | "global" | "builtin" | `project:${string}`;

export interface SkillsSection {
  id: string;
  title: string;
  skills: SkillInfo[];
}

// Mirrors crates/goose/src/skills/mod.rs::validate_skill_name.
// Keep in sync with the Rust rule.
const MAX_SKILL_NAME_LENGTH = 64;

export function isValidSkillName(name: string): boolean {
  return (
    name.length > 0 &&
    name.length <= MAX_SKILL_NAME_LENGTH &&
    !name.startsWith("-") &&
    !name.endsWith("-") &&
    [...name].every(
      (char) =>
        (char >= "a" && char <= "z") ||
        (char >= "0" && char <= "9") ||
        char === "-",
    )
  );
}

export function formatSkillName(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, "-")
    .replace(/^-/, "")
    .slice(0, MAX_SKILL_NAME_LENGTH);
}

export function uniqueProjectFilters(skills: SkillInfo[]) {
  const seen = new Map<string, string>();
  for (const skill of skills) {
    for (const project of skill.projectLinks) {
      if (!seen.has(project.id)) {
        seen.set(project.id, project.name);
      }
    }
  }
  return [...seen.entries()]
    .map(([id, name]) => ({ id, name }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

export function compareSkillsByName(a: SkillInfo, b: SkillInfo) {
  return (
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) ||
    a.name.localeCompare(b.name) ||
    a.path.localeCompare(b.path)
  );
}

export function filterSkills(
  skills: SkillInfo[],
  filters: {
    search: string;
    activeFilter: SkillsFilter;
  },
): SkillInfo[] {
  const searchTerm = filters.search.trim().toLowerCase();
  return skills.filter((skill) => {
    const matchesSearch =
      searchTerm.length === 0 ||
      skill.name.toLowerCase().includes(searchTerm) ||
      skill.description.toLowerCase().includes(searchTerm) ||
      skill.sourceLabel.toLowerCase().includes(searchTerm);

    const matchesFilter =
      filters.activeFilter === "all"
        ? true
        : filters.activeFilter === "global"
          ? skill.sourceKind === "global"
          : filters.activeFilter === "builtin"
            ? skill.sourceKind === "builtin"
            : skill.projectLinks.some(
                (project) => `project:${project.id}` === filters.activeFilter,
              );

    return matchesSearch && matchesFilter;
  });
}

export function groupSkills(
  filteredSkills: SkillInfo[],
  activeFilter: SkillsFilter,
  projectFilters: { id: string; name: string }[],
  labels: {
    personalTitle: string;
    builtinTitle: string;
    projectsFallback: string;
  },
): SkillsSection[] {
  if (activeFilter === "global") {
    return [
      {
        id: "personal",
        title: labels.personalTitle,
        skills: [...filteredSkills].sort(compareSkillsByName),
      },
    ];
  }

  if (activeFilter === "builtin") {
    return [
      {
        id: "builtin",
        title: labels.builtinTitle,
        skills: [...filteredSkills].sort(compareSkillsByName),
      },
    ];
  }

  if (activeFilter.startsWith("project:")) {
    const projectId = activeFilter.slice("project:".length);
    const projectName =
      projectFilters.find((project) => project.id === projectId)?.name ??
      labels.projectsFallback;

    return [
      {
        id: activeFilter,
        title: projectName,
        skills: [...filteredSkills].sort(compareSkillsByName),
      },
    ];
  }

  const personalSkills = filteredSkills
    .filter((skill) => skill.sourceKind === "global")
    .sort(compareSkillsByName);

  const builtinSkills = filteredSkills
    .filter((skill) => skill.sourceKind === "builtin")
    .sort(compareSkillsByName);

  const projectSections = projectFilters
    .map((project) => ({
      id: `project:${project.id}`,
      title: project.name,
      skills: filteredSkills
        .filter((skill) =>
          skill.projectLinks.some((link) => link.id === project.id),
        )
        .sort(compareSkillsByName),
    }))
    .filter((section) => section.skills.length > 0);

  return [
    ...(personalSkills.length > 0
      ? [
          {
            id: "personal",
            title: labels.personalTitle,
            skills: personalSkills,
          },
        ]
      : []),
    ...(builtinSkills.length > 0
      ? [
          {
            id: "builtin",
            title: labels.builtinTitle,
            skills: builtinSkills,
          },
        ]
      : []),
    ...projectSections,
  ];
}

export function downloadExport(json: string, filename: string) {
  const blob = new Blob([json], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  URL.revokeObjectURL(url);
}
