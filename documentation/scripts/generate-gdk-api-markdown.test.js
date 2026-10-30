const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const {
  generateReferences,
  renderMarkdown,
} = require("./generate-gdk-api-markdown");
const { LANGUAGES } = require("../src/components/GdkApiReference/api");
const data = require("../src/data/gdk-api.json");

test("publishes the linked APIs and preserves versioned snapshots", (t) => {
  const outputDir = fs.mkdtempSync(path.join(os.tmpdir(), "gdk-api-markdown-"));
  t.after(() => fs.rmSync(outputDir, { recursive: true, force: true }));
  const previous = { ...data.versions[0], version: "0.0.1", docVersion: "0.0" };
  const versions = [...data.versions, previous];
  generateReferences({ versions }, outputDir);

  const llms = fs.readFileSync(
    path.join(__dirname, "../static/llms.txt"),
    "utf8",
  );
  const links = [
    ...llms.matchAll(
      /\]\((\/docs\/gdk\/sdk\/api-reference-(?:python|kotlin)\.md)(?:#([^)]*))?\)/g,
    ),
  ];
  assert.ok(
    links.length > 0,
    "llms.txt must advertise the generated references",
  );
  for (const [, target, anchor] of links) {
    const content = fs.readFileSync(
      path.join(outputDir, path.basename(target)),
      "utf8",
    );
    if (anchor)
      assert.ok(
        content.includes(`id="${anchor}"`),
        `Missing ${target}#${anchor}`,
      );
  }

  for (const language of ["python", "kotlin"]) {
    const filename = `api-reference-${language}.md`;
    const latest = fs.readFileSync(path.join(outputDir, filename), "utf8");
    for (const [index, doc] of versions.entries()) {
      const snapshot = fs.readFileSync(
        path.join(outputDir, doc.version, filename),
        "utf8",
      );
      assert.ok(snapshot.includes(`goose-sdk ${doc.version}`));
      if (index === 0) assert.equal(latest, snapshot);
      else assert.notEqual(latest, snapshot);
      for (const func of doc.functions) {
        const anchor = `fn-${func.name.replaceAll("_", "-")}`;
        assert.ok(
          snapshot.includes(`id="${anchor}"`),
          `Missing function ${func.name}`,
        );
      }
      for (const item of doc.items) {
        assert.ok(snapshot.includes(`id="${item.name.toLowerCase()}"`));
        for (const method of item.methods) {
          const anchor = `${item.name}-${method.name}`
            .replaceAll("_", "-")
            .toLowerCase();
          assert.ok(
            snapshot.includes(`id="${anchor}"`),
            `Missing method ${item.name}.${method.name}`,
          );
        }
      }
    }
  }
});

test("renders binding signatures, defaults, error payloads, and field documentation", () => {
  const python = renderMarkdown(
    data.versions[0],
    LANGUAGES.find((entry) => entry.id === "python"),
  );
  assert.ok(
    python.includes(
      "async def Provider.stream(model: ProviderModelConfig, system: str, messages: list[ProviderMessage], tools: list[ProviderTool]) -> ProviderStream",
    ),
  );
  assert.ok(
    python.includes(
      "async def ProviderStream.next_chunk() -> StreamChunk | None",
    ),
  );
  assert.ok(
    python.includes(
      "async def Provider.compact(model_name: str, messages: list[CompactionMessage], templates: CompactionTemplates | None) -> CompactionSummary",
    ),
  );
  assert.ok(python.includes("`toolshim` | `bool` | `False`"));
  assert.ok(
    python.includes("`request_headers` | `dict[str, str] \\| None` | `None`"),
  );
  assert.ok(
    python.includes(
      "Per-request HTTP headers attached to the outgoing provider call.",
    ),
  );
  assert.ok(
    python.includes('id="gooseerror-variant-ratelimited-field-retry-after-ms"'),
  );
  assert.ok(python.includes("Raises `GooseError`."));
  assert.ok(!python.includes("<GdkApiReference"));

  const kotlin = renderMarkdown(
    data.versions[0],
    LANGUAGES.find((entry) => entry.id === "kotlin"),
  );
  assert.ok(
    kotlin.includes("suspend fun ProviderStream.nextChunk(): StreamChunk?"),
  );
  assert.ok(
    kotlin.includes(
      "suspend fun Provider.listModels(): List<ProviderModelInfo>",
    ),
  );
  assert.ok(
    kotlin.includes("`requestHeaders` | `Map<String, String>?` | `null`"),
  );
  assert.ok(kotlin.includes("@Throws(GooseException::class)"));
  assert.ok(kotlin.includes("### `GooseException.RateLimited`"));
});

test("Markdown exports turn static download links into fetchable paths", async (t) => {
  const siteDir = fs.mkdtempSync(
    path.join(os.tmpdir(), "gdk-markdown-export-"),
  );
  t.after(() => fs.rmSync(siteDir, { recursive: true, force: true }));
  fs.mkdirSync(path.join(siteDir, "docs"));
  fs.writeFileSync(
    path.join(siteDir, "docs/reference.mdx"),
    '---\ntitle: Reference\n---\nimport Api from "./api";\n\n[Python](pathname:///docs/gdk/sdk/api-reference-python.md)\n',
  );
  const plugin = require("../plugins/markdown-export.cjs")({ siteDir });
  await plugin.postBuild({ outDir: path.join(siteDir, "build") });
  assert.equal(
    fs.readFileSync(path.join(siteDir, "build/docs/reference.md"), "utf8"),
    "[Python](/docs/gdk/sdk/api-reference-python.md)",
  );
});
