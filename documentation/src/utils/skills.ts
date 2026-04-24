import type {
  Skill,
  SkillStatus,
  SkillInstallMethod,
  SupportingFilesType,
} from "@site/src/pages/skills/types";
import siteConfig from "@generated/docusaurus.config";

// Skills data is loaded from a generated JSON manifest at build time
// Generated at: documentation/static/skills-manifest.json

// Cache for loaded skills
let skillsCache: Skill[] | null = null;
let skillsPromise: Promise<Skill[]> | null = null;

/**
 * Get a skill by its ID
 */
export function getSkillById(id: string): Skill | null {
  const allSkills = loadAllSkillsSync();
  return allSkills.find((skill) => skill.id === id) || null;
}

/**
 * Search skills by query string
 * Searches name, description, and tags
 */
export async function searchSkills(query: string): Promise<Skill[]> {
  const allSkills = await loadAllSkills();
  if (!query) return allSkills;

  const lowerQuery = query.toLowerCase();
  return allSkills.filter(
    (skill) =>
      skill.name?.toLowerCase().includes(lowerQuery) ||
      skill.description?.toLowerCase().includes(lowerQuery) ||
      skill.tags?.some((tag) => tag.toLowerCase().includes(lowerQuery)),
  );
}

/**
 * Load all skills - async version that fetches from manifest
 */
export async function loadAllSkills(): Promise<Skill[]> {
  // Never fetch/cache during SSR (prevents "empty list" getting locked in on preview)
  if (typeof window === "undefined") return [];

  if (skillsCache) return skillsCache;
  if (skillsPromise) return skillsPromise;

  skillsPromise = fetchSkillsManifest();

  const skills = await skillsPromise;

  // Only cache if we actually got data (avoid caching [] due to a transient 404)
  if (skills.length > 0) skillsCache = skills;

  return skills;
}

/**
 * Load all skills synchronously (uses cache, returns empty if not loaded)
 */
export function loadAllSkillsSync(): Skill[] {
  if (skillsCache) return skillsCache;

  // Trigger async load on client
  if (typeof window !== "undefined") {
    void loadAllSkills();
  }

  return [];
}

/**
 * Fetch skills manifest from static files
 */
async function fetchSkillsManifest(): Promise<Skill[]> {
  try {
    // In Docusaurus, baseUrl changes automatically for PR previews.
    // Example:
    //   prod:      /
    //   PR preview: /pr-preview/pr-6752/
    const baseUrl = siteConfig.baseUrl.endsWith("/")
      ? siteConfig.baseUrl
      : `${siteConfig.baseUrl}/`;

    const manifestUrl = `${baseUrl}skills-manifest.json`;

    const response = await fetch(manifestUrl);
    if (!response.ok) {
      console.error(
        "Failed to fetch skills manifest:",
        response.status,
        manifestUrl,
      );
      return [];
    }

    const manifest = await response.json();
    return manifest.skills || [];
  } catch (error) {
    console.error("Error loading skills manifest:", error);
    return [];
  }
}

/**
 * Normalize raw frontmatter-like data to Skill type
 * (kept here in case you reuse it elsewhere)
 */
export function normalizeSkill(
  parsed: { frontmatter: Record<string, any>; content: string },
  id: string,
  supportingFiles: string[],
): Skill {
  const { frontmatter, content } = parsed;

  const sourceUrl = frontmatter.source_url || frontmatter.sourceUrl;
  const repoUrl = frontmatter.repo_url || frontmatter.repoUrl || sourceUrl;
  const author = frontmatter.author;
  const isCommunity = !!author && author.toLowerCase() !== "goose";

  const installMethod = determineInstallMethod(sourceUrl, id);
  const installCommand = generateInstallCommand(sourceUrl, id, installMethod);
  const supportingFilesType = determineSupportingFilesType(supportingFiles);

  return {
    id,
    name: frontmatter.name || id,
    description: frontmatter.description || "No description provided.",
    author,
    version: frontmatter.version,
    status: (frontmatter.status as SkillStatus) || "stable",
    tags: Array.isArray(frontmatter.tags) ? frontmatter.tags : [],
    sourceUrl,
    repoUrl,
    isCommunity,
    content,
    hasSupporting: supportingFiles.length > 0,
    supportingFiles,
    supportingFilesType,
    installMethod,
    installCommand,
    viewSourceUrl: generateViewSourceUrl(id),
  };
}

/**
 * Determine the supporting files type based on file contents
 */
function determineSupportingFilesType(
  supportingFiles: string[],
): SupportingFilesType {
  if (supportingFiles.length === 0) {
    return "none";
  }

  // Executable file extensions
  const executableExtensions = [
    ".sh",
    ".bash",
    ".zsh",
    ".ps1",
    ".bat",
    ".cmd",
    ".py",
    ".rb",
    ".js",
    ".mjs",
    ".ts",
  ];

  // Template-like patterns
  const templatePatterns = [
    /\.template\./i,
    /\.tmpl\./i,
    /\.tpl\./i,
    /template\./i,
    /\.example\./i,
    /\.sample\./i,
    /\.skeleton\./i,
    /\.stub\./i,
    /\.j2$/i,
    /\.jinja2?$/i,
    /\.mustache$/i,
    /\.hbs$/i,
    /\.handlebars$/i,
    /\.ejs$/i,
    /\.erb$/i,
  ];

  const hasExecutable = supportingFiles.some((file) => {
    const ext = file.substring(file.lastIndexOf(".")).toLowerCase();
    return executableExtensions.includes(ext);
  });

  if (hasExecutable) {
    return "scripts";
  }

  const hasTemplates = supportingFiles.some((file) => {
    return templatePatterns.some((pattern) => pattern.test(file));
  });

  if (hasTemplates) {
    return "templates";
  }

  return "multi-file";
}

/**
 * Determine the install method based on source URL
 */
function determineInstallMethod(
  sourceUrl: string | undefined,
  skillId: string,
): SkillInstallMethod {
  if (!sourceUrl) return "download";
  if (sourceUrl.includes("aaif-goose/goose")) return "npx-multi";

  const simpleRepoPattern = /^https:\/\/github\.com\/[^\/]+\/[^\/]+\/?$/;
  if (simpleRepoPattern.test(sourceUrl)) return "npx-single";

  return "npx-multi";
}

/**
 * Generate the install command based on method
 */
function generateInstallCommand(
  sourceUrl: string | undefined,
  skillId: string,
  method: SkillInstallMethod,
): string | undefined {
  if (method === "download" || !sourceUrl) return undefined;

  if (method === "npx-single") {
    const match = sourceUrl.match(/github\.com\/([^\/]+\/[^\/]+)/);
    if (match) return `npx skills add ${match[1]}`;
  }

  if (method === "npx-multi") {
    return `npx skills add ${sourceUrl} --skill ${skillId}`;
  }

  return undefined;
}

/**
 * Generate the view source URL for a skill in the Agent-Skills repo
 */
function generateViewSourceUrl(skillId: string): string {
  return `https://github.com/block/Agent-Skills/tree/main/${skillId}`;
}

/**
 * Get all unique tags from all skills (async)
 */
export async function getAllTags(): Promise<string[]> {
  const allSkills = await loadAllSkills();
  const tagSet = new Set<string>();

  allSkills.forEach((skill) => {
    skill.tags.forEach((tag) => tagSet.add(tag));
  });

  return Array.from(tagSet).sort();
}
