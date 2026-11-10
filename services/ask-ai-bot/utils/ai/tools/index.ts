import { tool } from "ai";
import { z } from "zod";
import { sourceUrl } from "../source";
import { listCodebaseFiles, searchCodebase } from "./codebase-search";
import { viewCodebaseFiles } from "./codebase-viewer";
import { getDocsIndex } from "./docs";
import {
  getGitHubItem,
  getGitHubItemComments,
  getGitHubRelease,
  searchGitHub,
} from "./github";

const paths = z.union([
  z.string().min(1),
  z.array(z.string().min(1)).min(1).max(3),
]);
const lineCount = z.number().int().min(1).max(300).default(120);

export const aiTools = {
  search_docs: tool({
    description:
      "Search documentation sections. Results include exact section IDs, matching excerpts, links, and deprecation notes.",
    inputSchema: z.object({
      query: z.string().min(1),
      limit: z.number().int().min(1).max(10).default(5),
    }),
    execute: async ({ query, limit }) =>
      getDocsIndex().searchDocs(query, limit),
  }),
  view_docs: tool({
    description:
      "Read section IDs returned by search_docs, or exact documentation file paths. Follow truncated sections using startLine.",
    inputSchema: z.object({
      filePaths: paths.describe(
        "Section ID(s), e.g. getting-started/providers.md:1523, or exact file path(s).",
      ),
      startLine: z
        .number()
        .int()
        .min(1)
        .optional()
        .describe(
          "1-based line number. Defaults to the section's start, or 1 for a file.",
        ),
      lineCount,
    }),
    execute: async ({ filePaths, startLine, lineCount }) =>
      getDocsIndex().viewDocs(filePaths, startLine, lineCount),
  }),
  search_codebase: tool({
    description:
      "Search Rust and UI code for exact errors, names, or regex patterns. Narrow directory when possible.",
    inputSchema: z.object({
      query: z.string().min(1),
      limit: z.number().int().min(1).max(30).default(10),
      scope: z.enum(["ui", "crates"]).optional(),
      directory: z
        .string()
        .optional()
        .describe(
          "Directory within ui/ or crates/, relative to the repository root.",
        ),
      regex: z.boolean().default(false),
    }),
    execute: async ({ query, limit, scope, directory, regex }) =>
      (await searchCodebase(query, limit, scope, directory, regex)).map(
        (item) => ({ ...item, url: sourceUrl(item.filePath, item.line) }),
      ),
  }),
  view_codebase: tool({
    description:
      "Read exact source file paths relative to the repo root, including AGENTS.md for project policy.",
    inputSchema: z.object({
      filePaths: paths,
      startLine: z.number().int().min(1).default(1),
      lineCount,
    }),
    execute: async ({ filePaths, startLine, lineCount }) =>
      viewCodebaseFiles(filePaths, startLine - 1, lineCount),
  }),
  list_codebase_files: tool({
    description: "List a repository directory to find exact source paths.",
    inputSchema: z.object({ directory: z.string().min(1) }),
    execute: async ({ directory }) => listCodebaseFiles(directory),
  }),
  search_github: tool({
    description:
      "Search goose issues and PRs. Supports GitHub qualifiers; use updated sorting for recent bug reports.",
    inputSchema: z.object({
      query: z.string().min(1),
      sort: z.enum(["created", "updated", "comments"]).optional(),
      order: z.enum(["asc", "desc"]).default("desc"),
      state: z.enum(["open", "closed", "all"]).default("all"),
      limit: z.number().int().min(1).max(20).default(5),
    }),
    execute: async ({ query, ...options }) =>
      (await searchGitHub(query, options)).map((item) => ({
        ...item,
        body: item.body.slice(0, 800),
        bodyTruncated: item.body.length > 800,
      })),
  }),
  get_github_issue_or_pr: tool({
    description:
      "Read an issue or PR with its latest comments, author associations, and direct source links. Merged does not mean released.",
    inputSchema: z.object({
      issueNumber: z.number().int().positive(),
      includeComments: z.boolean().default(true),
      commentLimit: z.number().int().min(1).max(30).default(10),
    }),
    execute: async ({ issueNumber, includeComments, commentLimit }) => {
      const item = await getGitHubItem(issueNumber);
      const comments =
        includeComments && item.comments
          ? await getGitHubItemComments(issueNumber, commentLimit)
          : [];
      return {
        ...item,
        body: item.body.slice(0, 6000),
        bodyTruncated: item.body.length > 6000,
        latestComments: comments,
        omittedComments: item.comments - comments.length,
      };
    },
  }),
  get_github_release: tool({
    description:
      "Read the latest stable goose release or an exact release tag to check published features and fixes.",
    inputSchema: z.object({
      tag: z
        .string()
        .optional()
        .describe(
          "Exact tag, e.g. v1.53.0. Omit for the latest stable release.",
        ),
    }),
    execute: async ({ tag }) => getGitHubRelease(tag),
  }),
};
