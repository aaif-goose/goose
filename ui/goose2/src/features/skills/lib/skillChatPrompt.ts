import type { SkillInfo } from "../api/skills";
import type { ChatSkillDraft } from "@/features/chat/types";

type SkillLike = Pick<SkillInfo, "name">;
type SkillDraftLike = Pick<ChatSkillDraft, "name">;
export type SkillCommandMatch<TSkill extends SkillLike = SkillLike> = {
  skill: TSkill;
  promptText: string;
  displayText: string;
};
const SKILL_INSTRUCTION_PREFIX = "Use these skills for this request:";

const RESERVED_SLASH_COMMANDS = new Set([
  "clear",
  "compact",
  "doctor",
  "prompt",
  "prompts",
  "skills",
]);

export function isReservedSlashCommand(command: string): boolean {
  return RESERVED_SLASH_COMMANDS.has(command.trim().toLowerCase());
}

export function formatSkillChatPrompt(
  skillName: string,
  taskText = "",
): string {
  const name = skillName.trim();
  const task = taskText.trimStart();
  if (!task) {
    return `Use the ${name} skill`;
  }
  return `Use the ${name} skill to ${task}`;
}

export function formatSkillDraftsChatPrompt(
  skills: SkillDraftLike[],
  taskText = "",
): string {
  if (skills.length === 0) {
    return taskText;
  }

  if (skills.length === 1) {
    return formatSkillChatPrompt(skills[0].name, taskText);
  }

  const skillNames = skills
    .map((skill) => skill.name.trim())
    .filter(Boolean)
    .join(", ");
  const task = taskText.trimStart();
  if (!task) {
    return `Use the ${skillNames} skills`;
  }
  return `Use the ${skillNames} skills to ${task}`;
}

export function formatSkillInstructionPrompt(skills: SkillDraftLike[]): string {
  const skillNames = skills
    .map((skill) => skill.name.trim())
    .filter(Boolean)
    .join(", ");
  return `${SKILL_INSTRUCTION_PREFIX} ${skillNames}.`;
}

export function parseSkillInstructionPrompt(text: string): string[] {
  const trimmed = text.trim();
  if (!trimmed.startsWith(SKILL_INSTRUCTION_PREFIX)) {
    return [];
  }

  return trimmed
    .slice(SKILL_INSTRUCTION_PREFIX.length)
    .trim()
    .replace(/[.。]+$/, "")
    .split(",")
    .map((name) => name.trim())
    .filter(Boolean);
}

export function toChatSkillDraft(
  skill: Pick<SkillInfo, "id" | "name" | "description" | "sourceLabel">,
): ChatSkillDraft {
  return {
    id: skill.id,
    name: skill.name,
    description: skill.description,
    sourceLabel: skill.sourceLabel,
  };
}

export function expandSkillSlashCommand(
  text: string,
  skills: SkillLike[],
): string | null {
  const match = resolveSkillSlashCommand(text, skills);
  return match?.promptText ?? null;
}

export function resolveSkillSlashCommand<TSkill extends SkillLike>(
  text: string,
  skills: TSkill[],
): SkillCommandMatch<TSkill> | null {
  const match = text.trimStart().match(/^\/(\S+)(?:\s+([\s\S]*))?$/);
  if (!match) {
    return null;
  }

  const command = match[1].toLowerCase();
  if (isReservedSlashCommand(command)) {
    return null;
  }

  const skill = skills.find(
    (candidate) => candidate.name.toLowerCase() === command,
  );
  if (!skill) {
    return null;
  }

  const displayText = match[2]?.trimStart() ?? "";
  return {
    skill,
    promptText: formatSkillChatPrompt(skill.name, displayText),
    displayText,
  };
}
