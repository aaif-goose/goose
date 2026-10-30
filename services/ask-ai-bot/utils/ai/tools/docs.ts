import fs from "fs";
import path from "path";
import MiniSearch from "minisearch";
import { getCodebaseDir } from "../source";

export interface DocSection {
  id: string;
  filePath: string;
  title: string;
  heading: string;
  content: string;
  startLine: number;
  endLine: number;
  webUrl: string;
  deprecated: boolean;
  versionNote: string;
}

const stopWords = new Set(
  "a an and are can do does for goose how i in is it me my of on the to what with you".split(
    " ",
  ),
);

class DocsIndex {
  private sections = new Map<string, DocSection>();
  private documents = new Map<string, string[]>();
  private search = new MiniSearch<DocSection>({
    fields: ["title", "heading", "content", "filePath"],
    tokenize: (text) => text.match(/[\p{L}\p{N}_]+/gu) ?? [],
    processTerm: (term) =>
      stopWords.has(term.toLowerCase()) ? null : term.toLowerCase(),
    searchOptions: { boost: { heading: 4, title: 2, filePath: 1.5 } },
  });

  constructor(directory: string) {
    const walk = (dir: string) => {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory() && !["assets", "docker"].includes(entry.name)) {
          walk(fullPath);
        } else if (entry.isFile() && /\.mdx?$/.test(entry.name)) {
          this.addDocument(
            path.relative(directory, fullPath),
            fs.readFileSync(fullPath, "utf8"),
          );
        }
      }
    };
    walk(directory);
    this.search.addAll([...this.sections.values()]);
  }

  private addDocument(filePath: string, markdown: string): void {
    const lines = markdown.split(/\r?\n/);
    this.documents.set(filePath, lines);
    const frontmatter =
      markdown.match(/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/)?.[0] ?? "";
    const scalar = (key: string) =>
      frontmatter
        .match(new RegExp(`^${key}:\\s*(.+)$`, "m"))?.[1]
        .trim()
        .replace(/^['"]|['"]$/g, "");
    const title =
      scalar("title") ?? path.basename(filePath, path.extname(filePath));
    const slug = scalar("slug");
    const segments = filePath.replace(/\.mdx?$/, "").split("/");
    if (
      segments.length > 1 &&
      (/^(index|readme)$/i.test(segments.at(-1)!) ||
        segments.at(-1) === segments.at(-2))
    )
      segments.pop();
    const webUrl = `https://goose-docs.ai/docs/${slug ? slug.replace(/^\//, "") : segments.join("/")}`;
    const versionNote =
      markdown
        .match(
          /::: *(?:caution|warning|danger) +Deprecated[^\n]*\n([\s\S]*?):::/i,
        )?.[1]
        .trim() ?? "";
    const headings: { line: number; heading: string; anchor: string }[] = [];
    const ancestors: string[] = [];
    const anchors = new Map<string, number>();
    let fence = "";
    let fenceLength = 0;
    const firstLine = frontmatter
      ? frontmatter.trimEnd().split(/\r?\n/).length
      : 0;

    for (let i = firstLine; i < lines.length; i++) {
      const marker = lines[i].match(/^ {0,3}(`{3,}|~{3,})/);
      if (marker) {
        if (!fence) {
          fence = marker[1][0];
          fenceLength = marker[1].length;
        } else if (
          marker[1][0] === fence &&
          marker[1].length >= fenceLength &&
          /^ {0,3}(`+|~+)\s*$/.test(lines[i])
        )
          fence = "";
        continue;
      }
      if (fence) continue;
      const match = lines[i].match(/^(#{1,6})\s+(.+?)\s*#*$/);
      if (!match) continue;
      const text = match[2]
        .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
        .replace(/<[^>]+>|[*`]/g, "");
      ancestors.length = match[1].length - 1;
      ancestors.push(text);
      const baseAnchor = text
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s_-]/gu, "")
        .replace(/\s/g, "-");
      const duplicate = anchors.get(baseAnchor) ?? 0;
      anchors.set(baseAnchor, duplicate + 1);
      headings.push({
        line: i,
        heading: ancestors.filter(Boolean).join(" > "),
        anchor: `${baseAnchor}${duplicate ? `-${duplicate}` : ""}`,
      });
    }

    if (headings[0]?.line !== firstLine)
      headings.unshift({ line: firstLine, heading: title, anchor: "" });
    for (let i = 0; i < headings.length; i++) {
      const { line, heading, anchor } = headings[i];
      const end = headings[i + 1]?.line ?? lines.length;
      const content = lines
        .slice(line, end)
        .filter((text) => !/^(import|export)\s/.test(text))
        .join("\n")
        .trim();
      if (!content) continue;
      const id = `${filePath}:${line + 1}`;
      this.sections.set(id, {
        id,
        filePath,
        title,
        heading,
        content,
        startLine: line + 1,
        endLine: end,
        webUrl: `${webUrl}${anchor ? `#${anchor}` : ""}`,
        deprecated: !!versionNote,
        versionNote,
      });
    }
  }

  searchDocs(
    query: string,
    limit = 5,
  ): Array<Omit<DocSection, "content"> & { preview: string }> {
    const results = this.search.search(query);
    results.sort((a, b) => {
      const score = (result: typeof a) =>
        result.score *
        (this.sections.get(String(result.id))!.deprecated ? 0.15 : 1);
      return score(b) - score(a);
    });
    return results.slice(0, limit).map((result) => {
      const section = this.sections.get(String(result.id))!;
      const terms = result.terms.map((term) => term.toLowerCase());
      const lines = section.content.split("\n");
      let bestLine = 0;
      let bestScore = -1;
      for (let i = 0; i < lines.length; i++) {
        const score = terms.filter((term) =>
          lines[i].toLowerCase().includes(term),
        ).length;
        if (score > bestScore) {
          bestLine = i;
          bestScore = score;
        }
      }
      const preview = lines
        .slice(Math.max(0, bestLine - 2), bestLine + 7)
        .join("\n")
        .slice(0, 800);
      const { content, ...metadata } = section;
      return { ...metadata, preview };
    });
  }

  viewDocs(ids: string | string[], startLine?: number, lineCount = 120) {
    return (Array.isArray(ids) ? ids : [ids]).map((id) => {
      const section = this.sections.get(id);
      const firstSection =
        section ??
        [...this.sections.values()].find((item) => item.filePath === id);
      if (!firstSection)
        throw new Error(
          `Unknown documentation path or section: ${id}. Use search_docs to find its exact ID.`,
        );
      const lines = this.documents.get(firstSection.filePath)!;
      const start = startLine ?? section?.startLine ?? 1;
      const lastLine = section?.endLine ?? lines.length;
      if (start < (section?.startLine ?? 1) || start > lastLine)
        throw new Error(
          `startLine must be within the requested document or section.`,
        );
      const end = Math.min(start + lineCount - 1, lastLine);
      return {
        ...firstSection,
        startLine: start,
        endLine: end,
        content: lines.slice(start - 1, end).join("\n"),
        truncated: end < lastLine,
      };
    });
  }
}

let index: DocsIndex | undefined;
export function getDocsIndex(): DocsIndex {
  return (index ??= new DocsIndex(
    process.env.DOCS_PATH ||
      path.join(getCodebaseDir(), "documentation", "docs"),
  ));
}
