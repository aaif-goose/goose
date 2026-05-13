import {
  test,
  expect,
  navigateToAgents,
  buildInitScript,
} from "./fixtures/tauri-mock";

test.describe("Agents view", () => {
  test("navigates to agents view from sidebar", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);

    await expect(page.locator("h1", { hasText: "Agents" })).toBeVisible();
    await expect(
      page.getByText("Custom agent configurations for specific workflows"),
    ).toBeVisible();
  });

  test("displays agent cards from mock data", async ({ tauriMocked: page }) => {
    await navigateToAgents(page);

    await expect(page.getByLabel("Agent: Solo")).toBeVisible();
    await expect(page.getByLabel("Agent: Scout")).toBeVisible();
    await expect(page.getByLabel("Agent: Code Reviewer")).toBeVisible();
  });

  test("shows Built-in badge on built-in agents", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);

    const soloCard = page.getByLabel("Agent: Solo");
    await expect(soloCard.getByText("Built-in")).toBeVisible();

    const reviewerCard = page.getByLabel("Agent: Code Reviewer");
    await expect(reviewerCard.getByText("Built-in")).not.toBeVisible();
  });

  test("shows create new agent button", async ({ tauriMocked: page }) => {
    await navigateToAgents(page);
    await expect(page.getByLabel("Create new agent")).toBeVisible();
  });

  test("opens create agent dialog via New Agent button", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);
    await page
      .getByRole("button", { name: "New Agent", exact: true })
      .first()
      .click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.locator("h2", { hasText: "New Agent" })).toBeVisible();
    await expect(dialog.getByPlaceholder("e.g. Code Reviewer")).toBeVisible();
    await expect(
      dialog.getByPlaceholder("You are a helpful assistant that..."),
    ).toBeVisible();
  });

  test("opens create agent dialog via plus card", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);
    await page.getByLabel("Create new agent").click();
    await expect(page.getByRole("dialog")).toBeVisible();
  });

  test("create dialog has disabled Create button when fields are empty", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);
    await page
      .getByRole("button", { name: "New Agent", exact: true })
      .first()
      .click();

    const dialog = page.getByRole("dialog");
    await expect(dialog.getByRole("button", { name: "Create" })).toBeDisabled();
  });

  test("create dialog enables Create button when name and prompt are filled", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);
    await page
      .getByRole("button", { name: "New Agent", exact: true })
      .first()
      .click();

    const dialog = page.getByRole("dialog");
    await dialog.getByPlaceholder("e.g. Code Reviewer").fill("Test Agent");
    await dialog
      .getByPlaceholder("You are a helpful assistant that...")
      .fill("You are a test agent");

    await expect(dialog.getByRole("button", { name: "Create" })).toBeEnabled();
  });

  test("closes create agent dialog via Close button", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);
    await page
      .getByRole("button", { name: "New Agent", exact: true })
      .first()
      .click();

    await expect(page.getByRole("dialog")).toBeVisible();
    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByRole("dialog")).not.toBeVisible();
  });

  test("clicking a custom agent card opens details with edit actions", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);
    await page.getByLabel("Agent: Code Reviewer").click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(
      dialog.locator("[data-slot='dialog-title']").filter({
        hasText: "Code Reviewer",
      }),
    ).toBeVisible();
    await expect(dialog.getByText(/^Provider$/)).toBeVisible();
    await expect(dialog.getByText("claude-sonnet-4-20250514")).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Edit" })).toBeVisible();
  });

  test("built-in agent opens read-only details with Duplicate button", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);
    await page.getByLabel("Agent: Solo").click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(
      dialog.locator("[data-slot='dialog-title']").filter({
        hasText: "Solo",
      }),
    ).toBeVisible();
    await expect(
      dialog.getByRole("button", { name: /Duplicate/ }),
    ).toBeVisible();
    await expect(
      dialog.getByRole("button", { name: "Edit" }),
    ).not.toBeVisible();
    await expect(
      dialog.getByRole("button", { name: "Create" }),
    ).not.toBeVisible();
    await expect(
      dialog.getByRole("button", { name: "Save Changes" }),
    ).not.toBeVisible();
  });

  test("custom agent card dropdown menu shows correct items", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);

    const card = page.getByLabel("Agent: Code Reviewer");
    await card.getByLabel("Agent options").click();

    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Edit" })).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Duplicate" }),
    ).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Export" })).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Delete" })).toBeVisible();
  });

  test("built-in agent dropdown menu does not show Edit or Delete", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);

    const card = page.getByLabel("Agent: Solo");
    await card.getByLabel("Agent options").click();

    const menu = page.getByRole("menu");
    await expect(menu).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Edit" }),
    ).not.toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Duplicate" }),
    ).toBeVisible();
    await expect(menu.getByRole("menuitem", { name: "Export" })).toBeVisible();
    await expect(
      menu.getByRole("menuitem", { name: "Delete" }),
    ).not.toBeVisible();
  });

  test("Delete triggers confirmation dialog", async ({ tauriMocked: page }) => {
    await navigateToAgents(page);

    const card = page.getByLabel("Agent: Code Reviewer");
    await card.getByLabel("Agent options").click();
    await page.getByRole("menuitem", { name: "Delete" }).click();

    await expect(
      page.getByText('Delete "Code Reviewer" permanently?'),
    ).toBeVisible();
    await expect(
      page.getByText(
        "This agent and its configuration will be permanently removed.",
      ),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Delete" })).toBeVisible();
  });

  test("Cancel in delete confirmation closes dialog", async ({
    tauriMocked: page,
  }) => {
    await navigateToAgents(page);

    const card = page.getByLabel("Agent: Code Reviewer");
    await card.getByLabel("Agent options").click();
    await page.getByRole("menuitem", { name: "Delete" }).click();
    await expect(
      page.getByText('Delete "Code Reviewer" permanently?'),
    ).toBeVisible();

    const confirmDialog = page.locator(".max-w-sm", {
      has: page.getByText('Delete "Code Reviewer" permanently?'),
    });
    await confirmDialog.getByRole("button", { name: "Cancel" }).click();

    await expect(
      page.getByText('Delete "Code Reviewer" permanently?'),
    ).not.toBeVisible();
    await expect(page.getByLabel("Agent: Code Reviewer")).toBeVisible();
  });

  test("search filters agents", async ({ tauriMocked: page }) => {
    await navigateToAgents(page);
    await page.getByPlaceholder("Search agents...").fill("Solo");

    await expect(page.getByLabel("Agent: Solo")).toBeVisible();
    await expect(page.getByLabel("Agent: Scout")).not.toBeVisible();
    await expect(page.getByLabel("Agent: Code Reviewer")).not.toBeVisible();

    await page.getByPlaceholder("Search agents...").clear();
    await expect(page.getByLabel("Agent: Solo")).toBeVisible();
    await expect(page.getByLabel("Agent: Scout")).toBeVisible();
    await expect(page.getByLabel("Agent: Code Reviewer")).toBeVisible();
  });

  test("empty agent state shows only create button", async ({
    tauriMocked: page,
  }) => {
    await page.addInitScript({
      content: buildInitScript({ personas: [], skills: [] }),
    });
    await navigateToAgents(page);

    await expect(page.getByLabel("Create new agent")).toBeVisible();
    await expect(page.getByLabel(/^Agent: /)).not.toBeVisible();
  });
});
