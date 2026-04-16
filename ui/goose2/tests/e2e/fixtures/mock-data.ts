/**
 * Mock data for Tauri IPC mocking in E2E tests.
 *
 * These objects conform to the TypeScript interfaces used by the app:
 *
 * Persona (from src/shared/types/agents.ts):
 *   { id, displayName, avatar?, systemPrompt, provider?, model?, isBuiltin, isFromDisk?, createdAt, updatedAt }
 *
 * SkillInfo (from src/features/skills/api/skills.ts):
 *   { name, description, instructions, path }
 */

const now = new Date().toISOString();

export const MOCK_PERSONAS = [
  {
    id: "builtin-solo",
    displayName: "Solo",
    systemPrompt: "You are a general-purpose assistant.",
    isBuiltin: true,
    createdAt: now,
    updatedAt: now,
  },
  {
    id: "builtin-scout",
    displayName: "Scout",
    systemPrompt: "You are a research assistant that finds information.",
    isBuiltin: true,
    createdAt: now,
    updatedAt: now,
  },
  {
    id: "custom-reviewer",
    displayName: "Code Reviewer",
    systemPrompt: "You review code for quality and best practices.",
    provider: "claude",
    model: "claude-sonnet-4-20250514",
    isBuiltin: false,
    createdAt: now,
    updatedAt: now,
  },
];

export const MOCK_PROJECTS = [
  {
    id: "project-alpha",
    name: "Alpha",
    description: "Test project Alpha",
    prompt: "",
    icon: null,
    color: "#3b82f6",
    order: 0,
    preferredProvider: null,
    preferredModel: null,
    workingDirs: ["/tmp/alpha"],
    useWorktrees: false,
    createdAt: now,
    updatedAt: now,
  },
  {
    id: "project-beta",
    name: "Beta",
    description: "Test project Beta",
    prompt: "",
    icon: null,
    color: "#10b981",
    order: 1,
    preferredProvider: null,
    preferredModel: null,
    workingDirs: ["/tmp/beta"],
    useWorktrees: false,
    createdAt: now,
    updatedAt: now,
  },
];

export const MOCK_SKILLS = [
  {
    name: "code-review",
    description: "Reviews code for quality and best practices",
    instructions:
      "When asked to review code, analyze the diff and provide feedback on code quality, potential bugs, and best practices.",
    path: "/mock/.agents/skills/code-review/SKILL.md",
  },
  {
    name: "test-writer",
    description: "Generates unit tests for given code",
    instructions:
      "When asked to write tests, generate comprehensive unit tests covering edge cases, happy paths, and error scenarios.",
    path: "/mock/.agents/skills/test-writer/SKILL.md",
  },
];
