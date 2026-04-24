import { defineConfig } from "vitest/config";

export default defineConfig({
  define: {
    __SCREENSHOT_DIR__: JSON.stringify(
      process.env.SCREENSHOT_DIR || "tests/app-e2e/screenshots",
    ),
    __SCREENSHOT_ON_FAILURE__: process.env.SCREENSHOT_ON_FAILURE !== "false",
  },
  test: {
    include: ["tests/app-e2e/**/*.test.ts"],
    testTimeout: 60000,
    hookTimeout: 10000,
  },
});
