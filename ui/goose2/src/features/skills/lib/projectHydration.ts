import type { ProjectInfo } from "@/features/projects/api/projects";
import type { SkillInfo } from "../api/skills";

function normalizeWorkingDirKey(workingDir: string) {
  const normalized = workingDir.trim().replace(/\\/g, "/");
  if (!normalized) {
    return "";
  }
  if (/^\/+$/.test(normalized)) {
    return "/";
  }
  if (/^[A-Za-z]:\/+$/.test(normalized)) {
    return `${normalized.slice(0, 2)}/`;
  }

  return normalized.replace(/\/+$/, "");
}

export function hydrateProjectNames(
  skills: SkillInfo[],
  projects: ProjectInfo[],
) {
  const projectsByWorkingDir = new Map<
    string,
    Pick<ProjectInfo, "id" | "name">
  >();

  for (const project of projects) {
    for (const workingDir of project.workingDirs) {
      const normalizedDir = normalizeWorkingDirKey(workingDir);
      if (!normalizedDir || projectsByWorkingDir.has(normalizedDir)) {
        continue;
      }
      projectsByWorkingDir.set(normalizedDir, {
        id: project.id,
        name: project.name,
      });
    }
  }

  return skills.map((skill) => {
    if (skill.sourceKind !== "project") {
      return skill;
    }

    const projectLinks = skill.projectLinks.map((project) => {
      const savedProject = projectsByWorkingDir.get(
        normalizeWorkingDirKey(project.workingDir),
      );
      return {
        ...project,
        id: savedProject?.id ?? project.id,
        name: savedProject?.name ?? project.name,
      };
    });

    return {
      ...skill,
      projectLinks,
      sourceLabel: projectLinks[0]?.name ?? skill.sourceLabel,
    };
  });
}
