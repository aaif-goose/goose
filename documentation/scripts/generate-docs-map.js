const fs = require('fs');
const path = require('path');
const matter = require('gray-matter');

const DOCS_DIR = path.join(__dirname, '..', 'docs');
const OUTPUT_FILE = path.join(__dirname, '..', 'static', 'goose-docs-map.md');

function getTitle(frontmatter, content) {
  if (frontmatter.title) {
    return frontmatter.title;
  }

  const lines = content.split('\n');
  for (const line of lines) {
    if (line.startsWith('# ')) {
      return line.slice(2).trim();
    }
  }

  return null;
}

// Extract H2-H6 headings as nested bullet list
const MIN_HEADING_LEVEL = 2;
const MAX_HEADING_LEVEL = 6;

function getHeadings(content) {
  const bullets = [];
  const headingPattern = new RegExp(`^(#{${MIN_HEADING_LEVEL},${MAX_HEADING_LEVEL}}) (.+)$`);

  for (const line of content.split('\n')) {
    const match = line.match(headingPattern);
    if (!match) continue;

    const level = match[1].length;
    const indent = '  '.repeat(level - MIN_HEADING_LEVEL);
    bullets.push(`${indent}* ${match[2]}`);
  }

  return bullets.join('\n');
}

async function main() {
  const { globby } = await import('globby');
  
  const sections = [
    { name: 'Getting Started', pattern: 'getting-started/*.{md,mdx}' },
    { name: 'Guides', pattern: 'guides/**/*.{md,mdx}' },
  ];

  let output = `# goose Documentation Map

> Auto-generated. Last updated: ${new Date().toISOString().split('T')[0]}

`;

  for (const section of sections) {
    const files = await globby(section.pattern, { cwd: DOCS_DIR });
    output += `## ${section.name}\n\n`;

    for (const file of files.sort()) {
      try {
        const raw = fs.readFileSync(path.join(DOCS_DIR, file), 'utf-8');
        const { data, content } = matter(raw);
        const title = getTitle(data, content);
        if (!title) {
          console.warn(`[generate-docs-map] Warning: No title found for ${file}, skipping`);
          continue;
        }
        const headings = getHeadings(content);
        const urlPath = `docs/${file.replace('.mdx', '.md')}`;

        output += `### [${title}](${urlPath})\n\n`;
        if (headings) output += `${headings}\n\n`;
        
      } catch (err) {
        console.warn(`[generate-docs-map] Warning: Could not process ${file}, skipping`, err);
      }
    }
  }

  output += `---\n\n> Full docs: https://goose-docs.ai/\n`;

  fs.writeFileSync(OUTPUT_FILE, output);
  console.log(`[generate-docs-map] Generated: ${OUTPUT_FILE}`);
}

// Run main if executed directly
if (require.main === module) {
  main();
}

module.exports = { getTitle, getHeadings };
