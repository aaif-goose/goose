const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const vm = require("node:vm");
const { loadSiteConfig } = require("@docusaurus/core/lib/server/config");
const redirectPlugin = require("@docusaurus/plugin-client-redirects").default;

for (const baseUrl of ["/", "/preview/"]) {
  test(`oi-kit redirects preserve old doc links, queries, and anchors under ${baseUrl}`, async (t) => {
    const siteDir = path.join(__dirname, "..");
    const { siteConfig } = await loadSiteConfig({ siteDir });
    const [, options] = siteConfig.plugins.find(
      (plugin) =>
        Array.isArray(plugin) &&
        plugin[0] === "@docusaurus/plugin-client-redirects",
    );
    const movedRoutes = fs.readdirSync(path.join(siteDir, "docs/oi-kit"), {
      recursive: true,
    })
      .filter((file) => /\.mdx?$/.test(file))
      .map((file) =>
        `/docs/oi-kit/${file.replace(/\.mdx?$/, "").replace(/index$/, "")}`,
      );
    movedRoutes.push("/blog/tags/oi-kit", "/blog/tags/oi-kit/page/2");
    const routes = [...new Set([
      ...movedRoutes,
      ...options.redirects.map(({ to }) =>
        new URL(to, "https://example.com").pathname,
      ),
    ])];
    const outDir = fs.mkdtempSync(path.join(os.tmpdir(), "oi-kit-redirects-"));
    t.after(() => fs.rmSync(outDir, { recursive: true, force: true }));
    await redirectPlugin({ siteConfig }, {
      ...options,
      fromExtensions: [],
      toExtensions: [],
    }).postBuild({
      outDir,
      baseUrl,
      siteConfig,
      routesPaths: routes.map((route) => baseUrl + route.slice(1)),
    });

    const redirects = movedRoutes.map((to) => [
      to.replace("/docs/oi-kit", "/docs/gdk")
        .replace("/blog/tags/oi-kit", "/blog/tags/gdk"),
      to,
    ]);
    redirects.push(
      ["/docs/gdk/acp/zed", "/docs/guides/zed"],
      ["/docs/gdk/sdk", "/docs/oi-kit/reference/"],
      ["/docs/oi-kit/sdk", "/docs/oi-kit/reference/"],
    );
    for (const [from, to] of redirects) {
      const html = fs.readFileSync(
        path.join(outDir, from, "index.html"),
        "utf8",
      );
      assert.ok(
        html.includes(`rel="canonical" href="${baseUrl + to.slice(1)}"`),
        from,
      );
      const location = { search: "?language=python", hash: "#provider-stream" };
      vm.runInNewContext(html.match(/<script>([\s\S]*?)<\/script>/)[1], {
        window: { location },
      });
      assert.equal(
        location.href,
        `${baseUrl + to.slice(1)}${location.search}${location.hash}`,
        from,
      );
    }
  });
}
