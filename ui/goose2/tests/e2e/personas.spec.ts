import {
  test,
  expect,
  navigateToPersonas,
  buildInitScript,
} from "./fixtures/tauri-mock";

test.describe("Personas view", () => {
  test("navigates to personas view from sidebar", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    // Assert heading, subtitle, and sections are visible
    await expect(page.locator("h1", { hasText: "Personas" })).toBeVisible();
    await expect(page.getByText("Custom persona configurations")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: "Active Agents" }),
    ).toBeVisible();
    await expect(page.getByText("No active agents")).toBeVisible();
  });

  test("displays persona cards from mock data", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    // All 3 persona cards should be visible with their aria-labels
    await expect(page.getByLabel("Persona: Solo")).toBeVisible();
    await expect(page.getByLabel("Persona: Scout")).toBeVisible();
    await expect(page.getByLabel("Persona: Code Reviewer")).toBeVisible();
  });

  test("shows Built-in badge on builtin personas", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    // Solo and Scout are builtin — their cards should contain "Built-in" text
    const soloCard = page.getByLabel("Persona: Solo");
    await expect(soloCard.getByText("Built-in")).toBeVisible();
    const reviewerCard = page.getByLabel("Persona: Code Reviewer");
    await expect(reviewerCard.getByText("Built-in")).not.toBeVisible();
  });

  test("shows create new persona button", async ({ tauriMocked: page }) => {
    await navigateToPersonas(page);
    await expect(page.getByLabel("Create new persona")).toBeVisible();
  });

  test("opens create persona dialog via New Persona button", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    await page
      .getByRole("button", { name: "New Persona", exact: true })
      .first()
      .click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(
      dialog.locator("h2", { hasText: "New Persona" }),
    ).toBeVisible();
    // Check form fields
    await expect(dialog.getByPlaceholder("e.g. Code Reviewer")).toBeVisible();
    await expect(
      dialog.getByPlaceholder("You are a helpful assistant that..."),
    ).toBeVisible();
  });

  test("opens create persona dialog via plus card", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    await page.getByLabel("Create new persona").click();
    await expect(page.getByRole("dialog")).toBeVisible();
  });

  test("create dialog has disabled Create button when fields are empty", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    await page
      .getByRole("button", { name: "New Persona", exact: true })
      .first()
      .click();
    const dialog = page.getByRole("dialog");
    // Create button should be disabled
    await expect(dialog.getByRole("button", { name: "Create" })).toBeDisabled();
  });

  test("create dialog enables Create button when name and prompt are filled", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    await page
      .getByRole("button", { name: "New Persona", exact: true })
      .first()
      .click();
    const dialog = page.getByRole("dialog");
    await dialog.getByPlaceholder("e.g. Code Reviewer").fill("Test Persona");
    await dialog
      .getByPlaceholder("You are a helpful assistant that...")
      .fill("You are a test persona");
    await expect(dialog.getByRole("button", { name: "Create" })).toBeEnabled();
  });

  test("closes create persona dialog via Close button", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    await page
      .getByRole("button", { name: "New Persona", exact: true })
      .first()
      .click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.getByRole("button", { name: "Close" }).click();
    await expect(page.getByRole("dialog")).not.toBeVisible();
  });

  test("opens edit dialog when clicking a custom persona card", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    await page.getByLabel("Persona: Code Reviewer").click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(
      dialog.locator("h2", { hasText: "Edit Persona" }),
    ).toBeVisible();
    // Fields should be pre-filled
    await expect(dialog.getByPlaceholder("e.g. Code Reviewer")).toHaveValue(
      "Code Reviewer",
    );
  });

  test("builtin persona opens read-only dialog with Duplicate button", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    await page.getByLabel("Persona: Solo").click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    // Header shows persona name for read-only
    await expect(dialog.locator("h2", { hasText: "Solo" })).toBeVisible();
    // Duplicate button instead of Create/Save
    await expect(
      dialog.getByRole("button", { name: /Duplicate/ }),
    ).toBeVisible();
    // Should NOT have Create or Save buttons
    await expect(
      dialog.getByRole("button", { name: "Create" }),
    ).not.toBeVisible();
    await expect(
      dialog.getByRole("button", { name: "Save Changes" }),
    ).not.toBeVisible();
  });

  test("persona card dropdown menu shows correct items", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    // Open dropdown for Code Reviewer (custom persona)
    const card = page.getByLabel("Persona: Code Reviewer");
    await card.getByLabel("Persona options").click();
    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Edit" })).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Duplicate" }),
    ).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Export" })).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Delete" })).toBeVisible();
  });

  test("builtin persona dropdown menu does not show Delete", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    const card = page.getByLabel("Persona: Solo");
    await card.getByLabel("Persona options").click();
    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Edit" })).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Duplicate" }),
    ).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Export" })).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Delete" }),
    ).not.toBeVisible();
  });

  test("Delete triggers confirmation dialog", async ({ tauriMocked: page }) => {
    await navigateToPersonas(page);
    const card = page.getByLabel("Persona: Code Reviewer");
    await card.getByLabel("Persona options").click();
    await page.getByRole("menuitem", { name: "Delete" }).click();
    // Confirmation dialog
    await expect(page.getByText("Delete persona?")).toBeVisible();
    await expect(
      page.getByText(/Are you sure you want to delete.*Code Reviewer/),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Delete" })).toBeVisible();
  });

  test("Cancel in delete confirmation closes dialog", async ({
    tauriMocked: page,
  }) => {
    await navigateToPersonas(page);
    const card = page.getByLabel("Persona: Code Reviewer");
    await card.getByLabel("Persona options").click();
    await page.getByRole("menuitem", { name: "Delete" }).click();
    await expect(page.getByText("Delete persona?")).toBeVisible();
    // Click Cancel within the delete confirmation dialog container
    await page
      .locator("text=Delete persona?")
      .locator("..")
      .locator("..")
      .getByRole("button", { name: "Cancel" })
      .click();
    await expect(page.getByText("Delete persona?")).not.toBeVisible();
    // Persona card should still be there
    await expect(page.getByLabel("Persona: Code Reviewer")).toBeVisible();
  });

  test("search filters personas", async ({ tauriMocked: page }) => {
    await navigateToPersonas(page);
    await page.getByPlaceholder("Search personas...").fill("Solo");
    await expect(page.getByLabel("Persona: Solo")).toBeVisible();
    await expect(page.getByLabel("Persona: Scout")).not.toBeVisible();
    await expect(page.getByLabel("Persona: Code Reviewer")).not.toBeVisible();
    // Clear search
    await page.getByPlaceholder("Search personas...").clear();
    await expect(page.getByLabel("Persona: Solo")).toBeVisible();
    await expect(page.getByLabel("Persona: Scout")).toBeVisible();
    await expect(page.getByLabel("Persona: Code Reviewer")).toBeVisible();
  });

  test("empty persona state shows only create button", async ({
    tauriMocked: page,
  }) => {
    // Override mock data with empty personas before navigation
    await page.addInitScript({
      content: buildInitScript({ personas: [], skills: [] }),
    });
    await navigateToPersonas(page);
    await expect(page.getByLabel("Create new persona")).toBeVisible();
    // No persona cards should be visible
    await expect(page.getByLabel(/^Persona: /)).not.toBeVisible();
  });
});
