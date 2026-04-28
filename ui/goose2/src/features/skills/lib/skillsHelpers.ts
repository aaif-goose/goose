import type { SkillInfo } from "../api/skills";
import { SKILL_CATEGORY_ORDER, type SkillViewInfo } from "./skillCategories";

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

export function uniqueSkillCategories(skills: SkillViewInfo[]) {
  return SKILL_CATEGORY_ORDER.filter((category) =>
    skills.some((skill) => skill.inferredCategory === category),
  );
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
