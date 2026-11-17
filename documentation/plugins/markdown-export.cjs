const fs = require('fs');
const path = require('path');

module.exports = function markdownExportPlugin(context, options) {
  const pluginOptions = {
    enabled: true,
    ...options,
  };

  return {
    name: 'markdown-export',
    
    async postBuild({ outDir }) {
      if (!pluginOptions.enabled) {
        return;
      }

      console.log('[markdown-export] Starting markdown export...');
      
      const { globby } = await import('globby');
      
      const docsDir = path.join(context.siteDir, 'docs');
      const outputDir = path.join(outDir, 'docs');
      
      // Get all markdown files
      const files = await globby('**/*.{md,mdx}', { cwd: docsDir });
      
      for (const file of files) {
        const inputPath = path.join(docsDir, file);
        const outputPath = path.join(outputDir, file.replace('.mdx', '.md'));
        
        // Ensure output subdirectory exists
        fs.mkdirSync(path.dirname(outputPath), { recursive: true });
        
        const content = fs.readFileSync(inputPath, 'utf-8');
        
        // Strip frontmatter and clean up
        const cleaned = stripFrontmatter(content);
        
        // Write the cleaned markdown alongside HTML files
        fs.writeFileSync(outputPath, cleaned);
      }
      
      console.log(`[markdown-export] Successfully exported ${files.length} markdown files to ${outputDir}`);
    },
  };
};

function stripFrontmatter(content) {
  // Remove YAML frontmatter (everything between --- at the start)
  const withoutFrontmatter = content.replace(/^---\s*\n[\s\S]*?\n---\s*\n/, '');
  
  // Clean up any remaining import statements (for .mdx files)
  const withoutImports = withoutFrontmatter.replace(/^import .+$/gm, '');
  
  // Remove excessive empty lines and trim
  return withoutImports
    .replace(/\]\(pathname:\/\/\//g, '](/')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}


