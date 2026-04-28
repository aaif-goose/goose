import type { SkillInfo } from "../api/skills";

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
