import type { SkillInfo } from "../api/skills";

const SKILL_CATEGORY_ORDER = [
  "design",
  "engineering",
  "quality",
  "research",
  "writing",
  "integrations",
  "operations",
  "productivity",
  "general",
] as const;

export type SkillCategory = (typeof SKILL_CATEGORY_ORDER)[number];

export interface SkillViewInfo extends SkillInfo {
  inferredCategory: SkillCategory;
}

const DESIGN_SLUGS = new Set([
  "adapt",
  "animate",
  "audit",
  "bolder",
  "clarify",
  "colorize",
  "critique",
  "delight",
  "distill",
  "frontend-design",
  "harden",
  "impeccable",
  "layout",
]);

const ENGINEERING_SLUGS = new Set([
  "cloning-squareup-repos",
  "create-pr",
  "plugin-creator",
  "skill-creator",
  "skill-installer",
]);

const QUALITY_SLUGS = new Set([
  "code-review",
  "create-app-e2e-test",
  "edge-case-finder",
]);

const RESEARCH_SLUGS = new Set([
  "agent-browser",
  "codesearch",
  "dev-guides",
  "eng-ai-chat",
  "go-link",
  "openai-docs",
]);

const WRITING_SLUGS = new Set(["ceo-weekly-update"]);

const OPERATIONS_SLUGS = new Set([
  "check-ci",
  "datadog",
  "github:gh-address-comments",
  "github:gh-fix-ci",
]);

const INTEGRATION_SLUGS = new Set([
  "excel",
  "gdrive",
  "github:github",
  "launchdarkly",
  "linear",
  "powerpoint",
]);

const PRODUCTIVITY_SLUGS = new Set(["grocery-list-organizer"]);

const CATEGORY_KEYWORDS: Record<SkillCategory, string[]> = {
  design: [
    "accessibility",
    "animation",
    "breakpoint",
    "color",
    "copy",
    "design",
    "frontend",
    "interface",
    "layout",
    "mobile",
    "motion",
    "polish",
    "responsive",
    "spacing",
    "theme",
    "typography",
    "ui",
    "ux",
    "visual",
  ],
  engineering: [
    "app",
    "build",
    "codebase",
    "create",
    "feature",
    "implement",
    "install",
    "plugin",
    "react",
    "repository",
    "rust",
    "scaffold",
    "skill",
    "typescript",
  ],
  quality: [
    "bug",
    "coverage",
    "edge case",
    "lint",
    "quality",
    "regression",
    "review",
    "test",
    "verify",
  ],
  research: [
    "browse",
    "discover",
    "docs",
    "documentation",
    "find",
    "guide",
    "investigate",
    "knowledge",
    "look up",
    "query",
    "read",
    "search",
  ],
  writing: [
    "copy",
    "document",
    "draft",
    "edit",
    "email",
    "message",
    "rewrite",
    "summary",
    "update",
    "write",
  ],
  integrations: [
    "drive",
    "excel",
    "extension",
    "github",
    "google docs",
    "google drive",
    "google sheets",
    "google slides",
    "launchdarkly",
    "linear",
    "powerpoint",
    "sheets",
    "slides",
  ],
  operations: [
    "buildkite",
    "canary",
    "ci",
    "flag",
    "incident",
    "kochiku",
    "log",
    "metric",
    "monitor",
    "observability",
    "release",
    "trace",
  ],
  productivity: [
    "grocery",
    "meal plan",
    "organize",
    "organizer",
    "shopping",
    "weekly update",
  ],
  general: [],
};

function normalizeText(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9:+\s-]/g, " ");
}

function keywordScore(haystack: string, keywords: string[]) {
  return keywords.reduce((score, keyword) => {
    if (!haystack.includes(keyword)) {
      return score;
    }

    return score + (keyword.includes(" ") ? 2 : 1);
  }, 0);
}

function inferCategoryFromSlug(slug: string): SkillCategory | null {
  if (DESIGN_SLUGS.has(slug)) {
    return "design";
  }

  if (QUALITY_SLUGS.has(slug)) {
    return "quality";
  }

  if (ENGINEERING_SLUGS.has(slug)) {
    return "engineering";
  }

  if (RESEARCH_SLUGS.has(slug)) {
    return "research";
  }

  if (WRITING_SLUGS.has(slug)) {
    return "writing";
  }

  if (OPERATIONS_SLUGS.has(slug)) {
    return "operations";
  }

  if (INTEGRATION_SLUGS.has(slug) || slug.startsWith("google-drive:")) {
    return "integrations";
  }

  if (PRODUCTIVITY_SLUGS.has(slug)) {
    return "productivity";
  }

  return null;
}

function inferSkillCategory(
  skill: Pick<SkillInfo, "name" | "description" | "instructions">,
): SkillCategory {
  const slug = skill.name.toLowerCase();
  const explicitCategory = inferCategoryFromSlug(slug);
  if (explicitCategory) {
    return explicitCategory;
  }

  const haystack = normalizeText(
    [skill.name, skill.description, skill.instructions].join(" "),
  );
  let bestCategory: SkillCategory = "general";
  let bestScore = 0;

  for (const category of SKILL_CATEGORY_ORDER) {
    if (category === "general") {
      continue;
    }

    const score = keywordScore(haystack, CATEGORY_KEYWORDS[category]);
    if (score > bestScore) {
      bestCategory = category;
      bestScore = score;
    }
  }

  return bestScore > 0 ? bestCategory : "general";
}

function withInferredSkillCategory(skill: SkillInfo): SkillViewInfo {
  return {
    ...skill,
    inferredCategory: inferSkillCategory(skill),
  };
}

export function withInferredSkillCategories(
  skills: SkillInfo[],
): SkillViewInfo[] {
  return skills.map(withInferredSkillCategory);
}

export function uniqueSkillCategories(
  skills: SkillViewInfo[],
): SkillCategory[] {
  return SKILL_CATEGORY_ORDER.filter((category) =>
    skills.some((skill) => skill.inferredCategory === category),
  );
}
