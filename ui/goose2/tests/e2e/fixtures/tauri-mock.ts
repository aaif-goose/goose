/**
 * Playwright custom fixture that injects a Tauri IPC mock into the page
 * before every navigation. This allows E2E tests to run against the frontend
 * without the real Tauri backend.
 *
 * Also installs a `window.WebSocket` stub for the ACP connection so features
 * like skills (which use `client.extMethod("_goose/sources/...")`) can run
 * without a live goose-acp server.
 */

import { test as base, expect, type Page } from "@playwright/test";
import { MOCK_PERSONAS, MOCK_PROJECTS, MOCK_SKILLS } from "./mock-data";

/**
 * Build the init script that will be injected into the page via
 * `page.addInitScript()`. The script sets up `window.__TAURI_INTERNALS__`
 * with an `invoke` handler that returns mock data for every Tauri command
 * the app is known to call, plus a WebSocket mock for ACP traffic.
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
      const FAKE_ACP_URL = "ws://127.0.0.1:0/mock-acp";

      // ------------------------------------------------------------------
      // ACP over a fake WebSocket. The frontend opens a ws connection
      // obtained from invoke("get_goose_serve_url") and then speaks
      // JSON-RPC 2.0 to the ACP SDK. We intercept that socket here and
      // respond to the handful of methods the app calls.
      // ------------------------------------------------------------------

      const skillToSourceEntry = (s) => ({
        type: "skill",
        name: s.name,
        description: s.description,
        content: s.instructions ?? s.content ?? "",
        directory: (s.path ?? ("/mock/.agents/skills/" + s.name + "/SKILL.md")).replace(/\\/SKILL\\.md$/, ""),
        global: true,
      });

      function handleAcpRequest(method, params) {
        switch (method) {
          case "initialize":
            return {
              protocolVersion: 1,
              agentCapabilities: {},
              authMethods: [],
            };
          case "_goose/sources/list":
            return { sources: SKILLS.map(skillToSourceEntry) };
          case "_goose/sources/create":
            return {
              source: {
                type: "skill",
                name: params?.name ?? "new-skill",
                description: params?.description ?? "",
                content: params?.content ?? "",
                directory: "/mock/.agents/skills/" + (params?.name ?? "new-skill"),
                global: params?.global ?? true,
              },
            };
          case "_goose/sources/update":
            return {
              source: {
                type: "skill",
                name: params?.name ?? "updated-skill",
                description: params?.description ?? "",
                content: params?.content ?? "",
                directory: "/mock/.agents/skills/" + (params?.name ?? "updated-skill"),
                global: params?.global ?? true,
              },
            };
          case "_goose/sources/delete":
            return {};
          case "_goose/sources/export":
            return {
              json: "{}",
              filename: (params?.name ?? "skill") + ".skill.json",
            };
          case "_goose/sources/import":
            return { sources: SKILLS.map(skillToSourceEntry) };
          default:
            // Unknown extension method — return empty object so callers
            // don't crash on unmocked fields.
            return {};
        }
      }

      class MockAcpWebSocket extends EventTarget {
        static CONNECTING = 0;
        static OPEN = 1;
        static CLOSING = 2;
        static CLOSED = 3;

        constructor(url) {
          super();
          this.url = url;
          this.readyState = 0;
          this.binaryType = "blob";
          // Dispatch open in a microtask so listeners can be registered.
          queueMicrotask(() => {
            this.readyState = 1;
            this.dispatchEvent(new Event("open"));
          });
        }

        send(data) {
          if (this.readyState !== 1) return;
          let msg;
          try {
            msg = typeof data === "string" ? JSON.parse(data) : null;
          } catch {
            return;
          }
          if (!msg || typeof msg !== "object") return;

          // Requests have both method and id; notifications have method
          // but no id; responses have id but no method. Only requests need
          // a reply.
          if (msg.method !== undefined && msg.id !== undefined) {
            let response;
            try {
              response = {
                jsonrpc: "2.0",
                id: msg.id,
                result: handleAcpRequest(msg.method, msg.params),
              };
            } catch (e) {
              response = {
                jsonrpc: "2.0",
                id: msg.id,
                error: { code: -32603, message: String(e) },
              };
            }
            queueMicrotask(() => {
              this.dispatchEvent(
                new MessageEvent("message", { data: JSON.stringify(response) }),
              );
            });
          }
        }

        close() {
          if (this.readyState === 3) return;
          this.readyState = 3;
          queueMicrotask(() => {
            this.dispatchEvent(new CloseEvent("close"));
          });
        }
      }

      const RealWebSocket = window.WebSocket;
      const PatchedWebSocket = function (url, protocols) {
        if (url === FAKE_ACP_URL) {
          return new MockAcpWebSocket(url);
        }
        return new RealWebSocket(url, protocols);
      };
      PatchedWebSocket.CONNECTING = 0;
      PatchedWebSocket.OPEN = 1;
      PatchedWebSocket.CLOSING = 2;
      PatchedWebSocket.CLOSED = 3;
      window.WebSocket = PatchedWebSocket;

      window.__TAURI_INTERNALS__ = {
        invoke(cmd, args) {
          switch (cmd) {
            // ---- ACP transport ----
            case "get_goose_serve_url":
              return Promise.resolve(FAKE_ACP_URL);

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
