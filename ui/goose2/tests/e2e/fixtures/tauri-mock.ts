/**
 * Playwright custom fixture that injects a Tauri IPC mock into the page
 * before every navigation. This allows E2E tests to run against the frontend
 * without the real Tauri backend.
 */

import { test as base, expect, type Page } from "@playwright/test";
import { MOCK_PERSONAS, MOCK_PROJECTS, MOCK_SKILLS } from "./mock-data";

/**
 * Build the init script that will be injected into the page via
 * `page.addInitScript()`. The script sets up `window.__TAURI_INTERNALS__`
 * with an `invoke` handler that returns mock data for every Tauri command
 * the app is known to call.
 *
 * Callers can override the default personas and skills arrays to test
 * empty-state or custom scenarios.
 */
export function buildInitScript(options?: {
  personas?: unknown[];
  skills?: unknown[];
  projects?: unknown[];
}): string {
  const personas = JSON.stringify(options?.personas ?? MOCK_PERSONAS);
  const skills = JSON.stringify(options?.skills ?? MOCK_SKILLS);
  const projects = JSON.stringify(options?.projects ?? MOCK_PROJECTS);

  return `
    (() => {
      const PERSONAS = ${personas};
      const SKILLS = ${skills};
      const PROJECTS = ${projects};

      window.__TAURI_INTERNALS__ = {
        invoke(cmd, args) {
          switch (cmd) {
            // ---- Personas ----
            case "list_personas":
              return Promise.resolve(PERSONAS);
            case "refresh_personas":
              return Promise.resolve(PERSONAS);
            case "create_persona":
              return Promise.resolve({
                id: "mock-" + Math.random().toString(36).slice(2, 10),
                displayName: args?.displayName ?? "New Persona",
                systemPrompt: args?.systemPrompt ?? "",
                isBuiltin: false,
                createdAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
                ...(args?.provider ? { provider: args.provider } : {}),
                ...(args?.model ? { model: args.model } : {}),
              });
            case "update_persona":
              return Promise.resolve({
                id: args?.id ?? "mock-updated",
                displayName: args?.displayName ?? "Updated Persona",
                systemPrompt: args?.systemPrompt ?? "",
                isBuiltin: false,
                createdAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
                ...(args?.provider ? { provider: args.provider } : {}),
                ...(args?.model ? { model: args.model } : {}),
              });
            case "delete_persona":
              return Promise.resolve(null);
            case "export_persona":
              return Promise.resolve({
                json: "{}",
                suggestedFilename: "persona.json",
              });
            case "import_personas":
              return Promise.resolve(PERSONAS);

            // ---- Skills ----
            case "list_skills":
              return Promise.resolve(SKILLS);
            case "create_skill":
              return Promise.resolve(null);
            case "update_skill":
              return Promise.resolve({
                name: args?.name ?? "updated-skill",
                description: args?.description ?? "",
                instructions: args?.instructions ?? "",
                path: "",
              });
            case "delete_skill":
              return Promise.resolve(null);
            case "export_skill":
              return Promise.resolve({
                json: "{}",
                filename: "skill.json",
              });
            case "import_skills":
              return Promise.resolve(SKILLS);

            // ---- Sessions / Misc ----
            case "list_sessions":
              return Promise.resolve([]);
            case "create_session":
              return Promise.resolve({
                id: "session-" + Math.random().toString(36).slice(2, 10),
                title: "New Chat",
                agentId: args?.agentId ?? null,
                projectId: args?.projectId ?? null,
                providerId: null,
                personaId: null,
                modelName: null,
                createdAt: new Date().toISOString(),
                updatedAt: new Date().toISOString(),
                archivedAt: null,
                messageCount: 0,
              });
            case "update_session":
              return Promise.resolve(null);
            case "get_session_messages":
              return Promise.resolve([]);
            case "archive_session":
              return Promise.resolve(null);
            case "list_projects":
              return Promise.resolve(PROJECTS);
            case "get_project":
              return Promise.resolve(PROJECTS.find(p => p.id === args?.id) ?? null);
            case "get_avatars_dir":
              return Promise.resolve("/tmp/avatars");
            case "save_persona_avatar_bytes":
              return Promise.resolve("avatar.png");
            case "discover_acp_providers":
              return Promise.resolve([]);
            case "list_files_for_mentions":
              return Promise.resolve([]);
            case "get_home_dir":
              return Promise.resolve("/tmp/home");
            case "path_exists":
              return Promise.resolve(false);

            // ---- Fallback ----
            default:
              console.warn("[tauri-mock] unhandled invoke command:", cmd, args);
              return Promise.resolve(null);
          }
        },

        transformCallback(callback, once) {
          return Math.floor(Math.random() * 1_000_000);
        },

        convertFileSrc(path) {
          return path;
        },
      };
    })();
  `;
}

// ---------------------------------------------------------------------------
// Playwright fixture
// ---------------------------------------------------------------------------

export const test = base.extend<{ tauriMocked: Page }>({
  tauriMocked: async ({ page }, use) => {
    await page.addInitScript({ content: buildInitScript() });
    await use(page);
  },
});

export { expect };

// ---------------------------------------------------------------------------
// Navigation helpers
// ---------------------------------------------------------------------------

export async function waitForHome(page: Page) {
  await expect(page.getByText(/Good (morning|afternoon|evening)/)).toBeVisible({
    timeout: 10_000,
  });
}

export async function navigateToPersonas(page: Page) {
  await page.goto("/");
  await expect(page.getByText(/Good (morning|afternoon|evening)/)).toBeVisible({
    timeout: 10_000,
  });
  await page.getByRole("button", { name: "Personas" }).click();
  await expect(page.locator("h1", { hasText: "Personas" })).toBeVisible();
}

export async function navigateToSkills(page: Page) {
  await page.goto("/");
  await expect(page.getByText(/Good (morning|afternoon|evening)/)).toBeVisible({
    timeout: 10_000,
  });
  await page.getByRole("button", { name: "Skills" }).click();
  await expect(page.locator("h1", { hasText: "Skills" })).toBeVisible();
}
