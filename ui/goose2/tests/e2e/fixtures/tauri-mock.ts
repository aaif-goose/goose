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
      const ACP_SESSIONS = [];

      function nowIso() {
        return new Date().toISOString();
      }

      function buildSession(sessionId, providerId = "goose") {
        return {
          sessionId,
          title: "New Chat",
          updatedAt: nowIso(),
          messageCount: 0,
          providerId,
          modelId: null,
        };
      }

      function findSession(sessionId) {
        return ACP_SESSIONS.find((session) => session.sessionId === sessionId) ?? null;
      }

      function jsonRpcResult(id, result) {
        return { jsonrpc: "2.0", id, result };
      }

      function handleAcpRequest(message) {
        switch (message.method) {
          case "initialize":
            return jsonRpcResult(message.id, {
              protocolVersion: "0.1.0",
              agentCapabilities: {
                loadSession: {},
                listSessions: {},
              },
              agentInfo: {
                name: "mock-goose",
                version: "0.0.0",
              },
              authMethods: [],
            });
          case "session/list":
            return jsonRpcResult(message.id, {
              sessions: ACP_SESSIONS.map((session) => ({
                sessionId: session.sessionId,
                title: session.title,
                updatedAt: session.updatedAt,
                _meta: {
                  messageCount: session.messageCount,
                },
              })),
            });
          case "session/new": {
            const providerId = message.params?.meta?.provider ?? "goose";
            const sessionId = "session-" + Math.random().toString(36).slice(2, 10);
            ACP_SESSIONS.unshift(buildSession(sessionId, providerId));
            return jsonRpcResult(message.id, { sessionId });
          }
          case "session/load":
            return jsonRpcResult(message.id, {});
          case "session/set_config_option": {
            const session = findSession(message.params?.sessionId);
            if (session) {
              if (message.params?.configId === "provider") {
                session.providerId = message.params?.value ?? session.providerId;
                session.modelId = null;
              }
              if (message.params?.configId === "model") {
                session.modelId = message.params?.value ?? null;
              }
              session.updatedAt = nowIso();
            }
            return jsonRpcResult(message.id, {});
          }
          case "session/prompt": {
            const session = findSession(message.params?.sessionId);
            if (session) {
              session.messageCount += 1;
              session.updatedAt = nowIso();
            }
            return jsonRpcResult(message.id, { stopReason: "end_turn" });
          }
          case "_goose/providers/list":
            return jsonRpcResult(message.id, { providers: [] });
          case "_goose/providers/inventory":
            return jsonRpcResult(message.id, { entries: [] });
          case "_goose/providers/inventory/refresh":
            return jsonRpcResult(message.id, { started: [], skipped: [] });
          case "_goose/working_dir/update":
          case "goose/working_dir/update":
            return jsonRpcResult(message.id, {});
          default:
            return jsonRpcResult(message.id, {});
        }
      }

      class MockWebSocket extends EventTarget {
        constructor(url) {
          super();
          this.url = url;
          this.readyState = 0;
          queueMicrotask(() => {
            this.readyState = 1;
            this.dispatchEvent(new Event("open"));
          });
        }

        send(raw) {
          const message = JSON.parse(raw);
          const response =
            message && typeof message === "object" && "id" in message
              ? handleAcpRequest(message)
              : null;
          if (!response) {
            return;
          }
          queueMicrotask(() => {
            this.dispatchEvent(
              new MessageEvent("message", {
                data: JSON.stringify(response),
              }),
            );
          });
        }

        close() {
          this.readyState = 3;
          this.dispatchEvent(new CloseEvent("close"));
        }
      }

      window.WebSocket = MockWebSocket;

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
              return Promise.resolve(
                ACP_SESSIONS.map((session) => ({
                  sessionId: session.sessionId,
                  title: session.title,
                  updatedAt: session.updatedAt,
                  messageCount: session.messageCount,
                })),
              );
            case "get_goose_serve_url":
              return Promise.resolve("ws://mock-goose");
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
            case "resolve_path": {
              const parts = args?.request?.parts ?? [];
              const path = parts
                .filter((part) => typeof part === "string" && part.length > 0)
                .join("/");
              const normalizedPath = path.startsWith("~/")
                ? "/tmp/home/" + path.slice(2)
                : path;
              return Promise.resolve({ path: normalizedPath });
            }

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
