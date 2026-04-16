import {
  test,
  expect,
  navigateToSkills,
  buildInitScript,
} from "./fixtures/tauri-mock";

test.describe("Skills view", () => {
  test("navigates to skills view from sidebar", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await expect(page.locator("h1", { hasText: "Skills" })).toBeVisible();
    await expect(
      page.getByText("Reusable instructions for your AI personas"),
    ).toBeVisible();
  });

  test("displays skills from mock data", async ({ tauriMocked: page }) => {
    await navigateToSkills(page);
    await expect(page.getByText("code-review")).toBeVisible();
    await expect(page.getByText("test-writer")).toBeVisible();
    await expect(
      page.getByText("Reviews code for quality and best practices"),
    ).toBeVisible();
    await expect(
      page.getByText("Generates unit tests for given code"),
    ).toBeVisible();
  });

  test("shows New Skill and Import buttons", async ({ tauriMocked: page }) => {
    await navigateToSkills(page);
    // The header has "New Skill" and "Import" buttons
    await expect(
      page.getByRole("button", { name: "New Skill" }).first(),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Import" })).toBeVisible();
  });

  test("opens create skill dialog from header button", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    // Click the first "New Skill" button (in the header)
    await page.getByRole("button", { name: "New Skill" }).first().click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.locator("h2", { hasText: "New Skill" })).toBeVisible();
    // Check form fields
    await expect(dialog.getByPlaceholder("my-skill-name")).toBeVisible();
    await expect(
      dialog.getByPlaceholder("What it does and when to use it..."),
    ).toBeVisible();
    await expect(
      dialog.getByPlaceholder("Markdown instructions the agent will follow..."),
    ).toBeVisible();
  });

  test("create skill dialog has disabled Create Skill button when empty", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByRole("button", { name: "New Skill" }).first().click();
    const dialog = page.getByRole("dialog");
    await expect(
      dialog.getByRole("button", { name: "Create Skill" }),
    ).toBeDisabled();
  });

  test("create skill dialog enables Create Skill when name and description filled", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByRole("button", { name: "New Skill" }).first().click();
    const dialog = page.getByRole("dialog");
    await dialog.getByPlaceholder("my-skill-name").fill("my-new-skill");
    await dialog
      .getByPlaceholder("What it does and when to use it...")
      .fill("A test skill");
    await expect(
      dialog.getByRole("button", { name: "Create Skill" }),
    ).toBeEnabled();
  });

  test("skill name auto-formats to kebab-case", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByRole("button", { name: "New Skill" }).first().click();
    const dialog = page.getByRole("dialog");
    const nameInput = dialog.getByPlaceholder("my-skill-name");
    // Type mixed case with spaces — should auto-format
    await nameInput.fill("My Skill Name");
    // The handleNameChange function lowercases and replaces non-alphanumeric with hyphens
    await expect(nameInput).toHaveValue("my-skill-name");
  });

  test("shows kebab-case validation error for trailing hyphen", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByRole("button", { name: "New Skill" }).first().click();
    const dialog = page.getByRole("dialog");
    // Type something that ends with a hyphen (the auto-formatter will produce "test-")
    await dialog.getByPlaceholder("my-skill-name").pressSequentially("test ");
    // The regex /^[a-z0-9]+(-[a-z0-9]+)*$/ won't match "test-", so error shows
    await expect(
      dialog.getByText("Must be kebab-case (e.g. code-review)"),
    ).toBeVisible();
  });

  test("closes skill dialog via Close button", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByRole("button", { name: "New Skill" }).first().click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.getByRole("button", { name: "Close" }).click();
    await expect(page.getByRole("dialog")).not.toBeVisible();
  });

  test("closes skill dialog via Cancel button", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByRole("button", { name: "New Skill" }).first().click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByRole("dialog")).not.toBeVisible();
  });

  test("skill options menu shows correct items", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByLabel("Options for code-review").click();
    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Edit" })).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Duplicate" }),
    ).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Export" })).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Delete" })).toBeVisible();
  });

  test("Edit opens edit dialog with pre-filled fields", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByLabel("Options for code-review").click();
    await page.getByRole("menuitem", { name: "Edit" }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.locator("h2", { hasText: "Edit Skill" })).toBeVisible();
    // Name should be pre-filled and read-only
    const nameInput = dialog.getByPlaceholder("my-skill-name");
    await expect(nameInput).toHaveValue("code-review");
    await expect(nameInput).toHaveAttribute("readonly", "");
  });

  test("Delete triggers confirmation dialog", async ({ tauriMocked: page }) => {
    await navigateToSkills(page);
    await page.getByLabel("Options for code-review").click();
    await page.getByRole("menuitem", { name: "Delete" }).click();
    await expect(page.getByText("Delete skill?")).toBeVisible();
    await expect(
      page.getByText(/Are you sure you want to delete.*code-review/),
    ).toBeVisible();
  });

  test("Cancel in delete confirmation closes dialog", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page.getByLabel("Options for code-review").click();
    await page.getByRole("menuitem", { name: "Delete" }).click();
    await expect(page.getByText("Delete skill?")).toBeVisible();
    // Find the Cancel button within the delete confirmation container
    const confirmDialog = page.locator(".max-w-sm", {
      has: page.getByText("Delete skill?"),
    });
    await confirmDialog.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByText("Delete skill?")).not.toBeVisible();
    // Skill should still be listed
    await expect(page.getByText("code-review")).toBeVisible();
  });

  test("search filters skills", async ({ tauriMocked: page }) => {
    await navigateToSkills(page);
    await page
      .getByPlaceholder("Search skills by name or description...")
      .fill("review");
    await expect(page.getByText("code-review")).toBeVisible();
    await expect(page.getByText("test-writer")).not.toBeVisible();
    // Clear search
    await page
      .getByPlaceholder("Search skills by name or description...")
      .clear();
    await expect(page.getByText("code-review")).toBeVisible();
    await expect(page.getByText("test-writer")).toBeVisible();
  });

  test("search with no results shows empty state", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);
    await page
      .getByPlaceholder("Search skills by name or description...")
      .fill("nonexistent-xyz");
    await expect(page.getByText("No matching skills")).toBeVisible();
    await expect(page.getByText("Try a different search term.")).toBeVisible();
  });

  test("empty skills state shows create prompt", async ({
    tauriMocked: page,
  }) => {
    // Override with empty skills — must be called BEFORE navigateToSkills
    await page.addInitScript({
      content: buildInitScript({ personas: [], skills: [] }),
    });
    await navigateToSkills(page);
    await expect(page.getByText("No skills yet")).toBeVisible();
    await expect(
      page.getByText("Create a skill or drop a .skill.json file here."),
    ).toBeVisible();
    // New Skill button in empty state
    await expect(
      page.getByRole("button", { name: "New Skill" }).first(),
    ).toBeVisible();
  });
});
