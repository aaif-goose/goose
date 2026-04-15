---
unlisted: true
---
# Writing Blog Posts for goose

This guide explains how to write and structure blog posts for the goose documentation site.

## Getting Started

1. Clone the goose repository:
```bash
git clone https://github.com/aaif-goose/goose.git
cd goose
```

2. Install dependencies:
```bash
cd documentation
npm install
```

## Directory Structure

Blog posts are organized by date using the following format:
```
YYYY-MM-DD-post-title/
├── index.md
└── images/
```

Example:
```
2025-05-22-llm-agent-readiness/
├── index.md
└── llm-agent-test.png
```

## Frontmatter

Each blog post must begin with YAML frontmatter that includes:

```yaml
---
title: Your Blog Post Title
description: A brief description of your post (1-2 sentences)
authors: 
    - your_author_id
---
```

The `authors` field should match your ID in the `authors.yml` file. Multiple authors can be listed. [More info on authors](#author-information).

## Header Image

After the frontmatter, include a header image using Markdown:

```markdown
![blog cover](your-image.png)
```

The header image should be:
- Relevant to the post content
- High quality (recommended dimensions: 1200 x 600 px)
- Stored in the post's directory
- Named descriptively

## Content Structure

### Introduction
Start with 1-2 paragraphs introducing the topic before the truncate tag. This will be what's shown on the blog index page.

### Truncate Tag
Add the truncate tag after your introduction to create a "read more" break:

```markdown
<!-- truncate -->
```

### Headers
Use headers to organize your content hierarchically:
- `#` (H1) - Used only for the post title in frontmatter
- `##` (H2) - Main sections
- `###` (H3) - Subsections
- `####` (H4) - Minor sections (these will not show on the right nav bar)

### Code Blocks
Use fenced code blocks with language specification:

````markdown
```javascript
// Your code here
```
````

### Images
Include additional images using Markdown:
```markdown
![descriptive alt text](image-name.png)
```

## Social Media Tags

At the end of your post, include the following meta tags for social media sharing:

```html
<head>
  <meta property="og:title" content="Your Blog Post Title" />
  <meta property="og:type" content="article" />
  <meta property="og:url" content="https://goose-docs.ai/blog/YYYY/MM/DD/post-slug" />
  <meta property="og:description" content="Your blog post description" />
  <meta property="og:image" content="https://goose-docs.ai/assets/images/your-image.png" />
  <meta name="twitter:card" content="summary_large_image" />
  <meta property="twitter:domain" content="goose-docs.ai" />
  <meta name="twitter:title" content="Your Blog Post Title" />
  <meta name="twitter:description" content="Your blog post description" />
  <meta name="twitter:image" content="https://goose-docs.ai/assets/images/your-image.png" />
</head>
```

## Author Information

To add yourself as an author:

1. Edit `authors.yml` in the blog directory
2. Add your information following this format:

```yaml
your_author_id:
  name: Your Full Name
  title: Your Title
  image_url: https://avatars.githubusercontent.com/u/your_github_id?v=4
  url: https://your-website.com  # Optional
  page: true
  socials:
    linkedin: your_linkedin_username
    github: your_github_username
    x: your_twitter_handle
    bluesky: your_bluesky_handle  # Optional
```

## Best Practices

1. **Writing Style**
   - Use clear, concise language
   - Break up long paragraphs
   - Include code examples where relevant
   - Use images to illustrate complex concepts

2. **Technical Content**
   - Include working code examples
   - Explain prerequisites
   - Link to relevant documentation
   - Test code snippets before publishing

3. **Formatting**
   - Use consistent spacing
   - Include alt text for images
   - Break up content with subheadings
   - Use lists and tables when appropriate

4. **Review Process**
   - Proofread for typos and grammar
   - Verify all links work
   - Check image paths
   - Test code samples
   - Validate frontmatter syntax

## Previewing Your Blog Post

To preview your blog post locally:

1. Ensure you're in the documentation directory:
```bash
cd documentation
```

2. Start the development server:
```bash
npm start
```

3. Open your browser and visit:
```
http://localhost:3000/blog
```

The development server features:
- Hot reloading (changes appear immediately)
- Preview of the full site navigation
- Mobile responsive testing
- Social media preview testing

If you make changes to your blog post while the server is running, the page will automatically refresh to show your updates.

### Troubleshooting Preview

If you encounter issues:

1. Make sure all dependencies are installed:
```bash
npm install
```

2. Clear the cache and restart:
```bash
npm run clear
npm start
```

3. Verify your frontmatter syntax is correct (no tabs, proper indentation)
4. Check that all image paths are correct relative to your post's directory
