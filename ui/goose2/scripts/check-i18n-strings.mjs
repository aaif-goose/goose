import { readFileSync, readdirSync } from "node:fs";
import { extname, join, relative } from "node:path";
import ts from "typescript";

const CHECKED_PATHS = [
  "src/app/ui",
  "src/features/agents",
  "src/features/chat/ui",
  "src/features/home",
  "src/features/projects",
  "src/features/settings",
  "src/features/skills",
  "src/features/sidebar",
  "src/features/status",
  "src/features/sessions",
  "src/shared/ui/ai-elements/code-block.tsx",
  "src/shared/ui/ai-elements/environment-variables.tsx",
  "src/shared/ui/ai-elements/message.tsx",
  "src/shared/ui/ai-elements/plan.tsx",
  "src/shared/ui/ai-elements/snippet.tsx",
  "src/shared/ui/ai-elements/stack-trace.tsx",
  "src/shared/ui/ai-elements/terminal.tsx",
  "src/shared/ui/ai-elements/context.tsx",
  "src/shared/ui/ai-elements/commit.tsx",
];

const EXCLUDED_PATH_SEGMENTS = ["__tests__"];
const EXCLUDED_FILE_MARKERS = [".test.", ".spec."];
const CHECKED_EXTENSIONS = new Set([".ts", ".tsx"]);
const TEXT_ATTRIBUTE_NAMES = new Set([
  "aria-label",
  "title",
  "placeholder",
  "alt",
]);
const TEXT_EXCLUDED_TAGS = new Set(["code", "pre", "kbd"]);
const IGNORE_COMMENT = "i18n-check-ignore";

function walkPath(targetPath) {
  const entries = [];
  let statEntries;
  try {
    statEntries = readdirSync(targetPath, { withFileTypes: true });
  } catch {
    if (CHECKED_EXTENSIONS.has(extname(targetPath))) {
      return [targetPath];
    }
    return [];
  }

  for (const entry of statEntries) {
    const fullPath = join(targetPath, entry.name);
    if (entry.isDirectory()) {
      entries.push(...walkPath(fullPath));
      continue;
    }

    if (CHECKED_EXTENSIONS.has(extname(entry.name))) {
      entries.push(fullPath);
    }
  }

  return entries;
}

function isExcluded(filePath) {
  const rel = relative(".", filePath);
  const normalizedRel = rel.replace(/\\/g, "/");
  const pathSegments = normalizedRel.split("/");

  return (
    EXCLUDED_PATH_SEGMENTS.some((segment) => pathSegments.includes(segment)) ||
    EXCLUDED_FILE_MARKERS.some((marker) => normalizedRel.includes(marker))
  );
}

function collapseWhitespace(text) {
  return text.replace(/\s+/g, " ").trim();
}

function isProbablyUserFacingText(text) {
  if (!text) return false;
  if (!/\p{L}/u.test(text)) return false;
  if (/^https?:\/\//.test(text)) return false;
  if (/^[./~@#][\w./-]+$/.test(text)) return false;
  if (/^[A-Z0-9_:-]+$/.test(text)) return false;
  if (/^[\w.-]+\.[A-Za-z]{2,}$/.test(text)) return false;
  return true;
}

function getLineText(sourceText, sourceFile, lineIndex) {
  if (lineIndex < 0) return "";

  const lineStarts = sourceFile.getLineStarts();
  if (lineIndex >= lineStarts.length) return "";

  const lineStart = lineStarts[lineIndex];
  const lineEnd =
    lineIndex + 1 < lineStarts.length
      ? lineStarts[lineIndex + 1]
      : sourceText.length;

  return sourceText.slice(lineStart, lineEnd);
}

function hasIgnoreComment(sourceText, sourceFile, node) {
  const start = node.getStart(sourceFile);
  const { line } = sourceFile.getLineAndCharacterOfPosition(start);

  return [line - 1, line].some((lineIndex) =>
    getLineText(sourceText, sourceFile, lineIndex).includes(IGNORE_COMMENT),
  );
}

function getParentTagName(node) {
  if (ts.isJsxElement(node.parent)) {
    return node.parent.openingElement.tagName.getText();
  }

  if (ts.isJsxSelfClosingElement(node.parent)) {
    return node.parent.tagName.getText();
  }

  return null;
}

function normalizeJsxText(text) {
  return collapseWhitespace(text);
}

function extractStringFromExpression(expression) {
  if (!expression) return null;

  if (ts.isStringLiteralLike(expression)) {
    return expression.text;
  }

  if (ts.isNoSubstitutionTemplateLiteral(expression)) {
    return expression.text;
  }

  if (ts.isTemplateExpression(expression)) {
    let text = expression.head.text;
    for (const span of expression.templateSpans) {
      text += `{${span.expression.getText()}}${span.literal.text}`;
    }
    return text;
  }

  return null;
}

function formatLocation(sourceFile, position) {
  const { line, character } =
    sourceFile.getLineAndCharacterOfPosition(position);
  return `${sourceFile.fileName}:${line + 1}:${character + 1}`;
}

function collectViolations(filePath) {
  const sourceText = readFileSync(filePath, "utf8");
  const sourceFile = ts.createSourceFile(
    filePath,
    sourceText,
    ts.ScriptTarget.Latest,
    true,
    filePath.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  const violations = [];

  function report(node, kind, text) {
    const normalizedText = collapseWhitespace(text);
    if (!isProbablyUserFacingText(normalizedText)) return;
    if (hasIgnoreComment(sourceText, sourceFile, node)) return;

    violations.push({
      location: formatLocation(sourceFile, node.getStart(sourceFile)),
      kind,
      text: normalizedText,
    });
  }

  function visit(node) {
    if (ts.isJsxText(node)) {
      const tagName = getParentTagName(node);
      if (tagName && TEXT_EXCLUDED_TAGS.has(tagName)) {
        return;
      }

      const text = normalizeJsxText(node.getText(sourceFile));
      report(node, "jsx-text", text);
    }

    if (ts.isJsxAttribute(node)) {
      const attributeName = node.name.text;
      if (TEXT_ATTRIBUTE_NAMES.has(attributeName) && node.initializer) {
        if (ts.isStringLiteral(node.initializer)) {
          report(
            node.initializer,
            `prop:${attributeName}`,
            node.initializer.text,
          );
        }

        if (ts.isJsxExpression(node.initializer)) {
          const text = extractStringFromExpression(node.initializer.expression);
          if (text) {
            report(node.initializer, `prop:${attributeName}`, text);
          }
        }
      }
    }

    if (ts.isJsxExpression(node) && node.expression) {
      if (ts.isJsxAttribute(node.parent)) {
        return;
      }

      const text = extractStringFromExpression(node.expression);
      if (text) {
        report(node, "jsx-expression", text);
      }
    }

    ts.forEachChild(node, visit);
  }

  visit(sourceFile);
  return violations;
}

const files = CHECKED_PATHS.flatMap(walkPath)
  .filter((filePath) => !isExcluded(filePath))
  .sort();

const violations = files.flatMap((filePath) => collectViolations(filePath));

if (violations.length > 0) {
  console.error("i18n string check failed:");
  for (const violation of violations) {
    console.error(
      `  - ${violation.location} [${violation.kind}] ${JSON.stringify(violation.text)}`,
    );
  }
  console.error("");
  console.error(
    `Wrap user-facing strings in translations or annotate a narrow exception with "${IGNORE_COMMENT}".`,
  );
  console.error(
    "The current enforcement scope is intentionally limited to app areas already migrated to i18n.",
  );
  process.exit(1);
} else {
  console.log("i18n string check passed.");
}
