import { test, expect, waitForHome } from "./fixtures/tauri-mock";

test.describe("Draft persistence", () => {
  test("home screen draft persists across navigation", async ({
    tauriMocked: page,
  }) => {
    await page.goto("/");
    await waitForHome(page);

    const input = page.getByLabel("Chat message input");
    await input.fill("my draft on home");

    // Wait for the 300ms debounce to persist the draft
    await page.waitForTimeout(500);

    // Navigate away to Personas
    await page.getByRole("button", { name: "Personas" }).click();
    await expect(page.locator("h1", { hasText: "Personas" })).toBeVisible();

    // Navigate back to Home
    await page.getByRole("button", { name: "Home" }).click();
    await waitForHome(page);

    // Draft should be restored
    await expect(page.getByLabel("Chat message input")).toHaveValue(
      "my draft on home",
    );
  });

  test("home screen draft clears after sending", async ({
    tauriMocked: page,
  }) => {
    await page.goto("/");
    await waitForHome(page);

    const input = page.getByLabel("Chat message input");
    await input.fill("send this message");

    // Send via Enter
    await input.press("Enter");

    // Should navigate to chat view — wait for chat input to appear without the draft
    await expect(page.getByLabel("Chat message input")).toHaveValue("");
  });

  test("chat draft persists when switching between project chats", async ({
    tauriMocked: page,
  }) => {
    await page.goto("/");
    await waitForHome(page);

    // Create a new chat in project Alpha
    await page.getByRole("button", { name: "Alpha" }).click();
    await page.getByTitle("New chat in project").first().click();

    // Type a draft
    const chatInput = page.getByLabel("Chat message input");
    await chatInput.fill("alpha draft");

    // Wait for debounce
    await page.waitForTimeout(500);

    // Navigate to Home, then create a chat in project Beta
    await page.getByRole("button", { name: "Home" }).click();
    await waitForHome(page);

    await page.getByRole("button", { name: "Beta" }).click();
    await page.getByTitle("New chat in project").last().click();
    await expect(page.getByLabel("Chat message input")).toHaveValue("");

    // Go back to project Alpha's draft via its + button
    await page.getByTitle("New chat in project").first().click();

    // Draft should be restored
    await expect(page.getByLabel("Chat message input")).toHaveValue(
      "alpha draft",
    );
  });

  test("draft sessions do not appear in sidebar", async ({
    tauriMocked: page,
  }) => {
    await page.goto("/");
    await waitForHome(page);

    // Expand the project in sidebar to see its chats
    await page.getByRole("button", { name: "Alpha" }).click();

    // Click the "+" for Alpha to create a new chat
    await page.getByTitle("New chat in project").first().click();

    // We should be in chat view now
    const chatInput = page.getByLabel("Chat message input");
    await expect(chatInput).toBeVisible();

    // The draft session should NOT appear in the sidebar
    const sidebar = page.locator("nav");
    await expect(sidebar.getByText("New Chat")).not.toBeVisible();
  });

  test("empty draft cleans up when navigating away", async ({
    tauriMocked: page,
  }) => {
    await page.goto("/");
    await waitForHome(page);

    // Open project and create a new chat
    await page.getByRole("button", { name: "Alpha" }).click();
    await page.getByTitle("New chat in project").first().click();

    // Don't type anything — leave it empty
    await expect(page.getByLabel("Chat message input")).toBeVisible();

    // Navigate away
    await page.getByRole("button", { name: "Home" }).click();
    await waitForHome(page);

    // Create another new chat in the same project
    await page.getByTitle("New chat in project").first().click();

    // Should get a fresh chat (the old empty one was cleaned up)
    await expect(page.getByLabel("Chat message input")).toHaveValue("");
  });

  test("draft with content is reused when clicking new chat again", async ({
    tauriMocked: page,
  }) => {
    await page.goto("/");
    await waitForHome(page);

    // Open project and create a new chat
    await page.getByRole("button", { name: "Alpha" }).click();
    await page.getByTitle("New chat in project").first().click();

    // Type a draft
    const chatInput = page.getByLabel("Chat message input");
    await chatInput.fill("work in progress");

    // Wait for debounce
    await page.waitForTimeout(500);

    // Click new chat in the same project again
    await page.getByTitle("New chat in project").first().click();

    // Should reuse the existing draft instead of creating a new one
    await expect(page.getByLabel("Chat message input")).toHaveValue(
      "work in progress",
    );
  });
});
