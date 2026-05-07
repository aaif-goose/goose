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
      const DISTRO = {
        present: false,
      };
      const FAKE_ACP_URL = "ws://127.0.0.1:0/mock-acp";
      const ACP_SESSIONS = [];
      const PROVIDER_INVENTORY = [
        {
          providerId: "claude",
          providerName: "Claude",
          description: "Claude provider",
          defaultModel: "claude-sonnet-4-20250514",
          configured: true,
          providerType: "Preferred",
          category: "model",
          configKeys: [],
          setupSteps: [],
          supportsRefresh: true,
          refreshing: false,
          lastUpdatedAt: null,
          lastRefreshAttemptAt: null,
          lastRefreshError: null,
          stale: false,
          modelSelectionHint: null,
          models: [
            {
              id: "claude-sonnet-4-20250514",
              name: "Claude Sonnet 4",
              family: "Claude",
              recommended: true,
            },
          ],
        },
        {
          providerId: "openai",
          providerName: "OpenAI",
          description: "OpenAI provider",
          defaultModel: "gpt-4.1",
          configured: true,
          providerType: "Preferred",
          category: "model",
          configKeys: [],
          setupSteps: [],
          supportsRefresh: true,
          refreshing: false,
          lastUpdatedAt: null,
          lastRefreshAttemptAt: null,
          lastRefreshError: null,
          stale: false,
          modelSelectionHint: null,
          models: [
            {
              id: "gpt-4.1",
              name: "GPT-4.1",
              family: "OpenAI",
              recommended: true,
            },
          ],
        },
      ];

      localStorage.setItem(
        "goose:onboarding:v1",
        JSON.stringify({
          completedAt: new Date().toISOString(),
          providerId: "openai",
          modelId: "gpt-4.1",
        }),
      );
      localStorage.setItem("goose:defaultProvider", "goose");
      localStorage.setItem(
        "goose:preferredModelsByAgent",
        JSON.stringify({
          goose: {
            providerId: "openai",
            modelId: "gpt-4.1",
            modelName: "GPT-4.1",
          },
        }),
      );

      const skillToSourceEntry = (s) => ({
        type: "skill",
        name: s.name,
        description: s.description,
        content: s.instructions ?? s.content ?? "",
        path: (s.path ?? ("/mock/.agents/skills/" + s.name + "/SKILL.md")).replace(/\\/SKILL\\.md$/, ""),
        global: s.global ?? true,
        supportingFiles: [],
      });

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
            return jsonRpcResult(message.id, { entries: PROVIDER_INVENTORY });
          case "_goose/providers/setup/catalog/list":
            return jsonRpcResult(message.id, { providers: [] });
          case "_goose/providers/inventory/refresh":
            return jsonRpcResult(message.id, { started: [], skipped: [] });
          case "_goose/defaults/read":
          case "_goose/defaults/save":
            return jsonRpcResult(message.id, {
              providerId: message.params?.providerId ?? "openai",
              modelId: message.params?.modelId ?? "gpt-4.1",
            });
          case "_goose/onboarding/import/scan":
            return jsonRpcResult(message.id, { candidates: [] });
          case "_goose/onboarding/import/apply":
            return jsonRpcResult(message.id, {
              imported: {
                providers: 0,
                extensions: 0,
                sessions: 0,
                skills: 0,
                projects: 0,
                preferences: 0,
              },
              skipped: {
                providers: 0,
                extensions: 0,
                sessions: 0,
                skills: 0,
                projects: 0,
                preferences: 0,
              },
              warnings: [],
            });
          case "_goose/working_dir/update":
          case "goose/working_dir/update":
            return jsonRpcResult(message.id, {});
          case "_goose/sources/list":
            return jsonRpcResult(message.id, { sources: SKILLS.map(skillToSourceEntry) });
          case "_goose/sources/create":
            return jsonRpcResult(message.id, {
              source: {
                name: message.params?.name ?? "new-skill",
                type: "skill",
                description: message.params?.description ?? "",
                content: message.params?.content ?? "",
                path: "/mock/.agents/skills/" + (message.params?.name ?? "new-skill"),
                global: message.params?.global ?? true,
              },
            });
          case "_goose/sources/update":
          case "goose/sources/update": {
            const path = message.params?.path ?? "/mock/.agents/skills/updated-skill";
            const nextName = message.params?.name;
            const name =
              typeof nextName === "string" && nextName.length > 0
                ? nextName
                : String(path).split("/").filter(Boolean).at(-1) ?? "updated-skill";
            const segments = String(path).split("/").filter(Boolean);
            if (segments.length > 0) {
              segments[segments.length - 1] = name;
            }
            const updatedPath = \`/\${segments.join("/")}\`;
            return jsonRpcResult(message.id, {
              source: {
                name,
                type: "skill",
                description: message.params?.description ?? "",
                content: message.params?.content ?? "",
                path: updatedPath,
                global: message.params?.global ?? true,
                supportingFiles: [],
              },
            });
          }
          case "_goose/sources/delete":
          case "goose/sources/delete":
            return jsonRpcResult(message.id, {});
          case "_goose/sources/export":
          case "goose/sources/export": {
            const path = message.params?.path ?? "/mock/.agents/skills/skill";
            const name = String(path).split("/").filter(Boolean).at(-1) ?? "skill";
            return jsonRpcResult(message.id, {
              json: "{}",
              filename: name + ".skill.json",
            });
          }
          case "_goose/sources/import":
            return jsonRpcResult(message.id, { sources: SKILLS.map(skillToSourceEntry) });
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
            // ---- ACP transport ----
            case "get_goose_serve_url":
              return Promise.resolve(FAKE_ACP_URL);
            case "get_distro_bundle":
              return Promise.resolve(DISTRO);

            // ---- Personas ----
            case "list_personas":
              return Promise.resolve(PERSONAS);
            case "refresh_personas":
              return Promise.resolve(PERSONAS);
            case "create_persona":
              return Promise.resolve({
                id: "mock-" + Math.random().toString(36).slice(2, 10),
                displayName: args?.displayName ?? "New Agent",
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
                displayName: args?.displayName ?? "Updated Agent",
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
              return Promise.resolve(
                ACP_SESSIONS.map((session) => ({
                  sessionId: session.sessionId,
                  title: session.title,
                  updatedAt: session.updatedAt,
                  messageCount: session.messageCount,
                })),
              );
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

export async function navigateToAgents(page: Page) {
  await page.goto("/");
  await expect(page.getByText(/Good (morning|afternoon|evening)/)).toBeVisible({
    timeout: 10_000,
  });
  await page.getByRole("button", { name: "Agents" }).click();
  await expect(page.locator("h1", { hasText: "Agents" })).toBeVisible();
}

export async function navigateToSkills(page: Page) {
  await page.goto("/");
  await expect(page.getByText(/Good (morning|afternoon|evening)/)).toBeVisible({
    timeout: 10_000,
  });
  await page.getByRole("button", { name: "Skills" }).click();
  await expect(page.locator("h1", { hasText: "Skills" })).toBeVisible();
}
