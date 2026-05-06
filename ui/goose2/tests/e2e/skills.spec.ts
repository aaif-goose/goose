import {
  test,
  expect,
  navigateToSkills,
  buildInitScript,
} from "./fixtures/tauri-mock";

test.describe("Skills view", () => {
  test("navigates to skills view and shows the redesigned header", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);

    await expect(page.locator("h1", { hasText: "Skills" })).toBeVisible();
    await expect(
      page.getByText(/Use skills to add specific instructions/),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "Import" })).toBeVisible();
    await expect(page.getByRole("button", { name: "New skill" })).toBeVisible();
  });

  test("shows skills in the list and opens a dedicated detail page", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);

    await expect(
      page.getByRole("button", { name: "Open layout details" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Open code-review details" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Open test-writer details" }),
    ).toBeVisible();

    await page
      .getByRole("button", { name: "Open test-writer details" })
      .click();

    await expect(
      page.getByRole("button", { name: "Back to skills" }),
    ).toBeVisible();
    await expect(page.getByText("alpha").first()).toBeVisible();
    await expect(
      page.getByText("/tmp/alpha/.goose/skills/test-writer/SKILL.md"),
    ).toBeVisible();
  });

  test("search filters the list", async ({ tauriMocked: page }) => {
    await navigateToSkills(page);

    await page.getByPlaceholder("Search skills").fill("review");

    await expect(page.getByText("code-review")).toBeVisible();
    await expect(page.getByText("test-writer")).not.toBeVisible();
  });

  test("project filtering isolates project skills", async ({
    tauriMocked: page,
  }) => {
    await navigateToSkills(page);

    await page
      .getByRole("main")
      .getByRole("button", { name: "Alpha", exact: true })
      .click();

    await expect(
      page.getByRole("button", { name: "Open test-writer details" }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "Open code-review details" }),
    ).not.toBeVisible();
  });

  test("opens the create skill dialog", async ({ tauriMocked: page }) => {
    await navigateToSkills(page);

    await page.getByRole("button", { name: "New Skill" }).click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.locator("h2", { hasText: "New Skill" })).toBeVisible();
    await expect(dialog.getByPlaceholder("my-skill-name")).toBeVisible();
    await expect(
      dialog.getByPlaceholder("What it does and when to use it..."),
    ).toBeVisible();
  });

  test("shows the empty state when no skills are available", async ({
    tauriMocked: page,
  }) => {
    await page.addInitScript({
      content: buildInitScript({ personas: [], projects: [], skills: [] }),
    });

    await navigateToSkills(page);

    await expect(page.getByText("No skills yet")).toBeVisible();
    await expect(
      page.getByText("Create a skill or import one to get started."),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "New Skill" })).toHaveCount(
      2,
    );
    await expect(page.getByRole("button", { name: "Import" })).toHaveCount(2);
  });
});
