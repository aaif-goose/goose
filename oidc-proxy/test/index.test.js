import {
  env,
  createExecutionContext,
  waitOnExecutionContext,
  fetchMock,
} from "cloudflare:test";
import { describe, it, expect, beforeAll, afterEach } from "vitest";
import worker from "../src/index.js";

let testKeyPair;
let testJwk;
const TEST_KID = "test-kid-001";

beforeAll(async () => {
  testKeyPair = await crypto.subtle.generateKey(
    {
      name: "RSASSA-PKCS1-v1_5",
      modulusLength: 2048,
      publicExponent: new Uint8Array([1, 0, 1]),
      hash: "SHA-256",
    },
    true,
    ["sign", "verify"],
  );
  const exported = await crypto.subtle.exportKey("jwk", testKeyPair.publicKey);
  testJwk = { ...exported, kid: TEST_KID, alg: "RS256", use: "sig" };
});

afterEach(() => {
  fetchMock.deactivate();
});

function base64UrlEncode(data) {
  const str = typeof data === "string" ? data : JSON.stringify(data);
  return btoa(str).replace(/=/g, "").replace(/\+/g, "-").replace(/\//g, "_");
}

async function createSignedJwt(payload, kid = TEST_KID) {
  const header = { alg: "RS256", typ: "JWT", kid };
  const headerB64 = base64UrlEncode(header);
  const payloadB64 = base64UrlEncode(payload);
  const data = new TextEncoder().encode(`${headerB64}.${payloadB64}`);
  const signature = await crypto.subtle.sign(
    "RSASSA-PKCS1-v1_5",
    testKeyPair.privateKey,
    data,
  );
  const sigB64 = btoa(String.fromCharCode(...new Uint8Array(signature)))
    .replace(/=/g, "")
    .replace(/\+/g, "-")
    .replace(/\//g, "_");
  return `${headerB64}.${payloadB64}.${sigB64}`;
}

let jtiCounter = 0;
function validPayload(overrides = {}) {
  const now = Math.floor(Date.now() / 1000);
  return {
    iss: "https://token.actions.githubusercontent.com",
    aud: "goose-oidc-proxy",
    iat: now - 10,
    exp: now + 300,
    jti: `test-jti-${++jtiCounter}`,
    repository: "block/goose",
    ref: "refs/heads/main",
    sub: "repo:block/goose:ref:refs/heads/main",
    ...overrides,
  };
}

function mockAll(upstreamStatus = 200, upstreamBody = { ok: true }) {
  fetchMock.activate();
  fetchMock.disableNetConnect();

  const oidc = fetchMock.get("https://token.actions.githubusercontent.com");
  oidc
    .intercept({ path: "/.well-known/openid-configuration", method: "GET" })
    .reply(
      200,
      JSON.stringify({
        jwks_uri:
          "https://token.actions.githubusercontent.com/.well-known/jwks",
      }),
    )
    .persist();
  oidc
    .intercept({ path: "/.well-known/jwks", method: "GET" })
    .reply(200, JSON.stringify({ keys: [testJwk] }))
    .persist();

  const upstream = fetchMock.get("https://api.anthropic.com");
  upstream
    .intercept({ path: /.*/, method: "POST" })
    .reply(upstreamStatus, JSON.stringify(upstreamBody));
}

function mockAllPersistent(upstreamStatus = 200, upstreamBody = { ok: true }) {
  fetchMock.activate();
  fetchMock.disableNetConnect();

  const oidc = fetchMock.get("https://token.actions.githubusercontent.com");
  oidc
    .intercept({ path: "/.well-known/openid-configuration", method: "GET" })
    .reply(
      200,
      JSON.stringify({
        jwks_uri:
          "https://token.actions.githubusercontent.com/.well-known/jwks",
      }),
    )
    .persist();
  oidc
    .intercept({ path: "/.well-known/jwks", method: "GET" })
    .reply(200, JSON.stringify({ keys: [testJwk] }))
    .persist();

  const upstream = fetchMock.get("https://api.anthropic.com");
  upstream
    .intercept({ path: /.*/, method: "POST" })
    .reply(upstreamStatus, JSON.stringify(upstreamBody))
    .persist();
}

// Mock TokenBucket Durable Object for unit tests
function mockTokenBucket(overrides = {}) {
  const defaults = { allowed: true, remaining: 199 };
  const response = { ...defaults, ...overrides };

  return {
    idFromName: () => "mock-id",
    get: () => ({
      fetch: async () => Response.json(response),
    }),
  };
}

function testEnv(overrides = {}) {
  return {
    OIDC_ISSUER: "https://token.actions.githubusercontent.com",
    OIDC_AUDIENCE: "goose-oidc-proxy",
    UPSTREAM_URL: "https://api.anthropic.com",
    UPSTREAM_AUTH_HEADER: "x-api-key",
    UPSTREAM_API_KEY: "sk-ant-real-key",
    ALLOWED_REPOS: "block/goose",
    MAX_TOKEN_AGE_SECONDS: "1200",
    MAX_REQUESTS_PER_TOKEN: "200",
    RATE_LIMIT_PER_SECOND: "2",
    TOKEN_BUCKET: mockTokenBucket(),
    ...overrides,
  };
}

describe("rejects invalid requests", () => {
  it("missing auth", async () => {
    const request = new Request("https://proxy.example.com/v1/messages");
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Missing authentication");
  });

  it("malformed token", async () => {
    const request = new Request("https://proxy.example.com/v1/messages", {
      headers: { "x-api-key": "not-a-jwt" },
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Malformed JWT");
  });

  it("wrong claims (repo, audience, issuer)", async () => {
    for (const [override, expectedError] of [
      [{ repository: "evil/repo" }, "not allowed"],
      [{ aud: "wrong" }, "Invalid audience"],
      [{ iss: "https://evil.example.com" }, "Invalid issuer"],
    ]) {
      const token = await createSignedJwt(validPayload(override));
      const request = new Request("https://proxy.example.com/v1/messages", {
        headers: { "x-api-key": token },
      });
      const ctx = createExecutionContext();
      const response = await worker.fetch(request, testEnv(), ctx);
      await waitOnExecutionContext(ctx);

      expect(response.status).toBe(401);
      expect((await response.json()).error).toContain(expectedError);
    }
  });

  it("token too old", async () => {
    const token = await createSignedJwt(
      validPayload({ iat: Math.floor(Date.now() / 1000) - 1500 }),
    );
    const request = new Request("https://proxy.example.com/v1/messages", {
      headers: { "x-api-key": token },
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token too old");
  });
});

describe("proxies valid requests", () => {
  it("forwards to upstream with injected API key", async () => {
    const token = await createSignedJwt(validPayload());
    mockAll(200, { id: "msg_123", type: "message" });

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: {
        "x-api-key": token,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ model: "claude-sonnet-4-20250514", messages: [] }),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(200);
    expect((await response.json()).id).toBe("msg_123");
  });

  it("accepts recently-expired token within MAX_TOKEN_AGE_SECONDS", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(
      validPayload({ iat: now - 600, exp: now - 300 }),
    );
    mockAll(200, { ok: true });

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(200);
  });
});

describe("token budget and rate limiting", () => {
  it("rejects when budget exhausted", async () => {
    const token = await createSignedJwt(validPayload());
    mockAll();

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(
      request,
      testEnv({
        TOKEN_BUCKET: mockTokenBucket({
          allowed: false,
          error: "budget_exhausted",
        }),
      }),
      ctx,
    );
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(429);
    expect((await response.json()).error).toBe("Token budget exhausted");
    expect(response.headers.get("Retry-After")).toBeNull();
  });

  it("rejects with Retry-After when rate limited", async () => {
    const token = await createSignedJwt(validPayload());
    mockAll();

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(
      request,
      testEnv({
        TOKEN_BUCKET: mockTokenBucket({
          allowed: false,
          error: "rate_limited",
        }),
      }),
      ctx,
    );
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(429);
    expect((await response.json()).error).toBe("Rate limit exceeded");
    expect(response.headers.get("Retry-After")).toBe("1");
  });
});
