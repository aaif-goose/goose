import { defineConfig, devices } from "@playwright/test";

const previewPort = 4173;

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 60_000,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: [
    ["list"],
    ["html", { open: "never", outputFolder: "playwright-report" }],
  ],
  use: {
    baseURL: `http://127.0.0.1:${previewPort}`,
    screenshot: "only-on-failure",
    trace: "on-first-retry",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "smoke",
      testMatch: ["**/smoke.spec.ts"],
      use: {
        ...devices["Desktop Chrome"],
      },
    },
    {
      name: "personas",
      testMatch: ["**/personas.spec.ts"],
      use: {
        ...devices["Desktop Chrome"],
      },
    },
    {
      name: "skills",
      testMatch: ["**/skills.spec.ts"],
      use: {
        ...devices["Desktop Chrome"],
      },
    },
    {
      name: "drafts",
      testMatch: ["**/drafts.spec.ts"],
      use: {
        ...devices["Desktop Chrome"],
      },
    },
  ],
  webServer: {
    // Opt-in reuse only. Reusing arbitrary local processes makes the suite
    // flaky when another test run or dev server happens to be bound here.
    command: `python3 -m http.server ${previewPort} -d dist`,
    cwd: ".",
    reuseExistingServer: process.env.PLAYWRIGHT_REUSE_SERVER === "1",
    url: `http://127.0.0.1:${previewPort}`,
  },
});
