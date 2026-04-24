import { beforeAll, beforeEach, afterAll, onTestFailed } from "vitest";
import { type TestDriver, createTestDriver } from "./test-driver-client";

declare const __SCREENSHOT_DIR__: string;
declare const __SCREENSHOT_ON_FAILURE__: boolean;

export const useTestDriver = (): TestDriver => {
  let inner: TestDriver;

  const testDriver = new Proxy({} as TestDriver, {
    get(_target, prop) {
      if (!inner)
        throw new Error("Test driver not connected — is beforeAll running?");
      return inner[prop as keyof TestDriver];
    },
  });

  beforeAll(async () => {
    inner = await createTestDriver();
  });

  afterAll(() => {
    inner?.close();
  });

  beforeEach(async () => {
    // Navigate to home before each test for clean state
    await inner.click('[data-testid="nav-home"]');

    if (__SCREENSHOT_ON_FAILURE__) {
      onTestFailed(async ({ task }) => {
        const name = task.name.replace(/\s+/g, "-").toLowerCase();
        const path = `${__SCREENSHOT_DIR__}/fail-${name}-${Date.now()}.png`;
        await inner.screenshot(path);
        console.log(`Screenshot saved: ${path}`);
      });
    }
  });

  return testDriver;
};
