import { tool } from "ai";
import { z } from "zod";
import { logger } from "../../logger";
import { listCodebaseFiles, searchCodebase } from "./codebase-search";
import { viewCodebaseFiles } from "./codebase-viewer";
import { searchDocs } from "./docs-search";
import { viewDocs } from "./docs-viewer";
import { getGitHubItem, getGitHubItemComments, searchGitHub } from "./github";

function truncateBody(body: string, maxLen: number = 500): string {
  return body.length > maxLen ? body.slice(0, maxLen) + "..." : body;
}

export const aiTools = {
  search_docs: tool({
    description: "Search the goose documentation for relevant information",
    inputSchema: z.object({
      query: z
        .string()
        .describe(
          "Search query for the documentation (example: 'sessions', 'tool management')",
        ),
      limit: z
        .number()
        .optional()
        .describe("Maximum number of results to return (default 15)"),
    }),
    execute: async ({ query, limit = 15 }) => {
      const results = searchDocs(query, limit);
      logger.verbose(
        `Searched docs for "${query}", found ${results.length} results`,
      );

      if (results.length === 0) {
        return "No relevant documentation found for your query. Try different keywords.";
      }

      return results
        .map(
          (r) =>
            `**${r.fileName}** (${r.filePath})\nPreview: ${r.preview}\nWeb URL: <${r.webUrl}>`,
        )
        .join("\n\n");
    },
  }),
  view_docs: tool({
    description: "View documentation file(s)",
    inputSchema: z.object({
      filePaths: z
        .union([z.string(), z.array(z.string())])
        .describe(
          "Path or array of paths to documentation files (example: 'quickstart.md' or ['guides/managing-projects.md', 'mcp/asana-mcp.md'])",
        ),
      startLine: z
        .number()
        .optional()
        .describe("Starting line number (0-indexed, default 0)"),
      lineCount: z
        .number()
        .optional()
        .describe("Number of lines to show (default 1500)"),
    }),
    execute: async ({ filePaths, startLine = 0, lineCount = 1500 }) => {
      try {
        const result = viewDocs(filePaths, startLine, lineCount);
        const count = Array.isArray(filePaths) ? filePaths.length : 1;
        logger.verbose(`Viewed ${count} documentation file(s)`);
        return result;
      } catch (error) {
        const errorMsg =
          error instanceof Error ? error.message : "Unknown error";
        logger.error(`Error viewing docs: ${errorMsg}`);
        return `Error viewing documentation: ${errorMsg}`;
      }
    },
  }),
  search_codebase: tool({
    description:
      "Search the goose source code (Rust crates and TypeScript UI) using regex patterns. Searches across ui/ and crates/. Use this to find function definitions, struct/type definitions, imports, error messages, or any code pattern.",
    inputSchema: z.object({
      query: z
        .string()
        .describe(
          "Regex pattern to search for in the codebase (example: 'fn create_session', 'struct Provider', 'impl.*Agent')",
        ),
      limit: z
        .number()
        .optional()
        .describe("Maximum number of results to return (default 20)"),
      scope: z
        .string()
        .optional()
        .describe(
          "Limit search to a specific area: 'ui' for the desktop and other UIs, 'crates' for Rust backend code. Omit to search everything.",
        ),
    }),
    execute: async ({ query, limit = 20, scope }) => {
      try {
        const results = searchCodebase(query, limit, scope);

        if (results.length === 0) {
          return "No matches found in the codebase. Try a different pattern or broader search.";
        }

        return results
          .map(
            (r) => `**${r.filePath}:${r.line}**\n\`\`\`\n${r.context}\n\`\`\``,
          )
          .join("\n\n");
      } catch (error) {
        const errorMsg =
          error instanceof Error ? error.message : "Unknown error";
        logger.error(`Error searching codebase: ${errorMsg}`);
        return `Error searching codebase: ${errorMsg}`;
      }
    },
  }),
  view_codebase: tool({
    description:
      "View source code file(s) from the goose codebase. Paths are relative to the repository root (e.g., 'crates/goose/src/agents/agent.rs' or 'ui/desktop/src/App.tsx').",
    inputSchema: z.object({
      filePaths: z
        .union([z.string(), z.array(z.string())])
        .describe(
          "Path or array of paths to source files relative to the repo root (example: 'crates/goose/src/agents/agent.rs' or ['ui/desktop/src/main.ts', 'crates/goose/src/acp/server.rs'])",
        ),
      startLine: z
        .number()
        .optional()
        .describe("Starting line number (0-indexed, default 0)"),
      lineCount: z
        .number()
        .optional()
        .describe(
          "Number of lines to show (default 200). Use smaller values for focused reading, larger for overview.",
        ),
    }),
    execute: async ({ filePaths, startLine = 0, lineCount = 200 }) => {
      try {
        const result = viewCodebaseFiles(filePaths, startLine, lineCount);
        const count = Array.isArray(filePaths) ? filePaths.length : 1;
        logger.verbose(`Viewed ${count} codebase file(s)`);
        return result;
      } catch (error) {
        const errorMsg =
          error instanceof Error ? error.message : "Unknown error";
        logger.error(`Error viewing codebase: ${errorMsg}`);
        return `Error viewing codebase: ${errorMsg}`;
      }
    },
  }),
  list_codebase_files: tool({
    description:
      "List files and directories in a codebase directory. Use this to explore the project structure before viewing specific files. Only works within ui/ and crates/.",
    inputSchema: z.object({
      directory: z
        .string()
        .describe(
          "Directory path relative to repo root (example: 'crates/goose/src', 'ui/desktop/src/components')",
        ),
    }),
    execute: async ({ directory }) => {
      try {
        const entries = listCodebaseFiles(directory);

        if (entries.length === 0) {
          return `Directory "${directory}" is empty.`;
        }

        return entries
          .map((e) => `${e.isDirectory ? "[dir] " : "      "}${e.filePath}`)
          .join("\n");
      } catch (error) {
        const errorMsg =
          error instanceof Error ? error.message : "Unknown error";
        logger.error(`Error listing codebase files: ${errorMsg}`);
        return `Error listing files: ${errorMsg}`;
      }
    },
  }),
  search_github: tool({
    description:
      "Search GitHub issues and pull requests in the aaif-goose/goose repository. Use this to find bugs, feature requests, or discussions. Results can be sorted by recency, relevance, or comment count.",
    inputSchema: z.object({
      query: z
        .string()
        .describe(
          "Search query (supports GitHub qualifiers like 'label:bug', 'is:issue', 'is:pr', 'author:username')",
        ),
      sort: z
        .enum(["created", "updated", "comments"])
        .optional()
        .describe(
          "Sort by created date, last updated, or comment count. Omit for relevance-based sorting (default).",
        ),
      order: z
        .enum(["asc", "desc"])
        .optional()
        .describe("Sort order (default: desc)"),
      state: z
        .enum(["open", "closed", "all"])
        .optional()
        .describe("Filter by issue state (default: all)"),
      limit: z
        .number()
        .optional()
        .describe("Maximum number of results (default 10)"),
    }),
    execute: async ({ query, sort, order, state, limit }) => {
      try {
        const results = await searchGitHub(query, {
          sort,
          order,
          state,
          limit,
        });

        if (results.length === 0) {
          return "Nothing found on GitHub matching your query. Try different keywords.";
        }

        return results
          .map((r) => {
            const status =
              r.state === "closed"
                ? r.isMerged
                  ? "merged"
                  : "closed"
                : r.state;
            return (
              `**#${r.number}** (${status}) - ${r.title}\n` +
              `Author: ${r.author} | Created: ${r.createdAt.slice(0, 10)} | Comments: ${r.comments}\n` +
              `Labels: ${r.labels.join(", ") || "none"}\n` +
              `${r.body ? truncateBody(r.body) + "\n" : ""}` +
              `URL: <${r.url}>`
            );
          })
          .join("\n\n");
      } catch (error) {
        const errorMsg =
          error instanceof Error ? error.message : "Unknown error";
        logger.error(`Error searching GitHub: ${errorMsg}`);
        return `Error searching GitHub: ${errorMsg}`;
      }
    },
  }),
  get_github_issue_or_pr: tool({
    description:
      "Get detailed information about a specific GitHub issue or pull request in the aaif-goose/goose repository, including its description and comments.",
    inputSchema: z.object({
      issueNumber: z.number().describe("The issue or pull request number"),
      includeComments: z
        .boolean()
        .optional()
        .describe(
          "Whether to include comments in the response (default: true)",
        ),
      commentLimit: z
        .number()
        .optional()
        .describe("Maximum number of comments to fetch (default: 30)"),
    }),
    execute: async ({
      issueNumber,
      includeComments = true,
      commentLimit = 30,
    }) => {
      try {
        const item = await getGitHubItem(issueNumber);

        const status =
          item.state === "closed"
            ? item.isMerged
              ? "merged"
              : "closed"
            : item.state;
        let result =
          `## #${item.number}: ${item.title}\n` +
          `**State:** ${status} | **Author:** ${item.author}\n` +
          `**Created:** ${item.createdAt} | **Updated:** ${item.updatedAt}\n` +
          `**Labels:** ${item.labels.join(", ") || "none"}\n` +
          `**URL:** <${item.url}>\n\n`;

        if (item.body) {
          result += `**Description:**\n${item.body.slice(0, 4000)}\n`;
        }

        if (includeComments && item.comments > 0) {
          const comments = await getGitHubItemComments(
            issueNumber,
            commentLimit,
          );
          result += `\n**Comments (${comments.length}):**\n`;
          result += comments
            .slice(0, commentLimit)
            .map(
              (c) =>
                `\n**${c.author}** (${c.createdAt.slice(0, 10)}):\n${c.body.slice(0, 1500)}`,
            )
            .join("\n---");
          if (item.comments > comments.length) {
            result += `\n\n... and ${item.comments - comments.length} more comments. Use get_github_issue_or_pr with includeComments=true and a larger commentLimit to fetch more.`;
          }
        }

        return result;
      } catch (error) {
        const errorMsg =
          error instanceof Error ? error.message : "Unknown error";
        logger.error(`Error getting GitHub item: ${errorMsg}`);
        return `Error getting item: ${errorMsg}`;
      }
    },
  }),
};
