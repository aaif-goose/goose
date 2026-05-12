import { expect, test } from "./fixtures/tauri-mock";

test.describe("Smoke tests", () => {
  test("app loads and shows home screen", async ({ tauriMocked: page }) => {
    await page.goto("/");

    // Wait for the app to render — greeting should appear
    await expect(
      page.getByText(/Good (morning|afternoon|evening)/),
    ).toBeVisible({ timeout: 10_000 });
  });

  test("home screen shows clock", async ({ tauriMocked: page }) => {
    await page.goto("/");

    // Should show AM or PM once the clock renders
    await expect(page.getByText(/[AP]M/)).toBeVisible({ timeout: 10_000 });
  });

  test("home screen shows chat input placeholder", async ({
    tauriMocked: page,
  }) => {
    await page.goto("/");

    await expect(
      page.getByPlaceholder(/Chat with .* or @ mention an agent/),
    ).toBeVisible({ timeout: 10_000 });
  });
});
