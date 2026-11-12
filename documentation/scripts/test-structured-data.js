#!/usr/bin/env node

/**
 * Checks the schema.org JSON-LD in a production build.
 * Run `npm run build` first, then `npm run test-structured-data`.
 * Kept out of `npm test`, which CI runs before the build exists.
 */

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const buildDir = path.join(__dirname, '..', 'build');
const articleTypes = new Set(['Article', 'TechArticle', 'BlogPosting', 'NewsArticle']);

if (!fs.existsSync(path.join(buildDir, 'index.html'))) {
  console.error(`No build found at ${buildDir}. Run \`npm run build\` first.`);
  process.exit(1);
}

function listHtmlFiles(dir) {
  return fs.readdirSync(dir, {withFileTypes: true}).flatMap((entry) => {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) return listHtmlFiles(fullPath);
    return entry.name.endsWith('.html') ? [fullPath] : [];
  });
}

function routeOf(file) {
  const relative = path.relative(buildDir, file).split(path.sep).join('/');
  return `/${relative.replace(/(^|\/)index\.html$/, '').replace(/\.html$/, '')}`;
}

function typesOf(node) {
  const type = node['@type'];
  if (!type) return [];
  return Array.isArray(type) ? type : [type];
}

// The theme renders its JSON-LD through react-helmet, which marks the tag with data-rh.
function readBlocks(html) {
  const blocks = [];
  for (const [, attributes, body] of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)) {
    if (!attributes.includes('type="application/ld+json"')) continue;
    const block = {fromTheme: attributes.includes('data-rh="true"'), topLevel: [], error: null};
    try {
      const data = JSON.parse(body);
      block.topLevel = Array.isArray(data['@graph']) ? data['@graph'] : [data];
    } catch (error) {
      block.error = error.message;
    }
    blocks.push(block);
  }
  return blocks;
}

function visit(value, callback) {
  if (Array.isArray(value)) {
    value.forEach((item) => visit(item, callback));
  } else if (value && typeof value === 'object') {
    callback(value);
    Object.values(value).forEach((item) => visit(item, callback));
  }
}

function readOgType(html) {
  const matches = [...html.matchAll(/<meta\b[^>]*property="og:type"[^>]*content="([^"]*)"/g)];
  return matches.length > 0 ? matches[matches.length - 1][1] : null;
}

const pages = listHtmlFiles(buildDir).map((file) => {
  const html = fs.readFileSync(file, 'utf8');
  const blocks = readBlocks(html);
  const topLevel = blocks.flatMap((block) => block.topLevel);
  const themeNodes = blocks.filter((block) => block.fromTheme).flatMap((block) => block.topLevel);
  const siteGraph = blocks.filter((block) => !block.fromTheme).flatMap((block) => block.topLevel);
  return {route: routeOf(file), ogType: readOgType(html), blocks, topLevel, themeNodes, siteGraph};
});

const countOf = (nodes, type) => nodes.filter((node) => typesOf(node).includes(type)).length;
const findNode = (nodes, type) => nodes.find((node) => typesOf(node).includes(type));
const pageAt = (route) => pages.find((page) => page.route === route);

function assertNoRoutes(routes, message) {
  assert.equal(routes.length, 0, `${message}: ${routes.slice(0, 5).join(', ')}`);
}

test('every JSON-LD block is valid JSON', () => {
  assertNoRoutes(
    pages.filter((page) => page.blocks.some((block) => block.error)).map((page) => page.route),
    'Invalid JSON-LD',
  );
});

test('no page has more than one BreadcrumbList', () => {
  assertNoRoutes(
    pages.filter((page) => countOf(page.topLevel, 'BreadcrumbList') > 1).map((page) => page.route),
    'Duplicate BreadcrumbList',
  );
});

test('no page describes its article twice', () => {
  const duplicated = pages.filter(
    (page) => page.topLevel.filter((node) => typesOf(node).some((type) => articleTypes.has(type))).length > 1,
  );
  assertNoRoutes(duplicated.map((page) => page.route), 'More than one article node');
});

test('every @id reference resolves on the same page', () => {
  const broken = [];
  for (const page of pages) {
    const defined = new Set();
    const referenced = new Set();
    visit(page.topLevel, (node) => {
      if (typeof node['@id'] !== 'string') return;
      (Object.keys(node).length === 1 ? referenced : defined).add(node['@id']);
    });
    const missing = [...referenced].filter((id) => !defined.has(id));
    if (missing.length > 0) broken.push(`${page.route} -> ${missing[0]}`);
  }
  assertNoRoutes(broken, 'Unresolved @id');
});

test('no @id has a doubled slash in its path', () => {
  const malformed = [];
  for (const page of pages) {
    visit(page.topLevel, (node) => {
      const id = node['@id'];
      if (typeof id === 'string' && id.replace(/^https?:\/\//, '').includes('//')) malformed.push(id);
    });
  }
  assertNoRoutes(malformed, 'Malformed @id');
});

test('every page with the site graph has one WebPage, WebSite and Organization', () => {
  const withGraph = pages.filter((page) => page.siteGraph.length > 0);
  assert.ok(withGraph.length > 0, 'No page carries the site graph');
  for (const type of ['WebPage', 'WebSite', 'Organization']) {
    assertNoRoutes(
      withGraph.filter((page) => countOf(page.siteGraph, type) !== 1).map((page) => page.route),
      `Expected exactly one ${type}`,
    );
  }
});

test('docs pages are TechArticle nodes with a headline and a modified date', () => {
  const articles = pages
    .map((page) => ({route: page.route, article: findNode(page.siteGraph, 'TechArticle')}))
    .filter(({article}) => article);
  assert.ok(articles.length > 0, 'No TechArticle found');
  assertNoRoutes(
    articles
      .filter(({article}) => !article.headline || Number.isNaN(Date.parse(article.dateModified)))
      .map(({route}) => route),
    'TechArticle without a headline or a valid dateModified',
  );
});

test('blog posts keep the theme BlogPosting and link it from the WebPage', () => {
  // The plugin only links pages that declare themselves articles through og:type.
  const posts = pages
    .map((page) => ({page, post: findNode(page.themeNodes, 'BlogPosting')}))
    .filter(({page, post}) => post && page.ogType === 'article');
  assert.ok(posts.length > 0, 'No BlogPosting found');
  assertNoRoutes(
    posts
      .filter(({page, post}) => findNode(page.siteGraph, 'WebPage')?.mainEntity?.['@id'] !== post['@id'])
      .map(({page}) => page.route),
    'WebPage.mainEntity does not reference the BlogPosting',
  );
});

test('home page describes the site, its publisher and the source repository', () => {
  const home = pageAt('/');
  for (const type of ['WebPage', 'WebSite', 'Organization', 'SoftwareSourceCode']) {
    assert.equal(countOf(home.siteGraph, type), 1, `Home page should have one ${type}`);
  }
});

test('installation page describes goose as a SoftwareApplication', () => {
  const installation = pageAt('/docs/getting-started/installation');
  assert.ok(installation, 'Installation page not found in the build');
  assert.equal(countOf(installation.siteGraph, 'SoftwareApplication'), 1);
  assert.equal(countOf(installation.topLevel, 'BreadcrumbList'), 1);
});
