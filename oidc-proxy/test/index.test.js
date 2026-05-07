import {
  env,
  createExecutionContext,
  waitOnExecutionContext,
  fetchMock,
} from "cloudflare:test";
import { describe, it, expect, beforeAll, afterEach, vi } from "vitest";
import worker, { TokenBucket } from "../src/index.js";

let testKeyPair;
let testJwk;
let ecKeyPair;
let ecJwk;
const TEST_KID = "test-kid-001";
const TEST_EC_KID = "test-ec-kid-001";

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

  ecKeyPair = await crypto.subtle.generateKey(
    {
      name: "ECDSA",
      namedCurve: "P-256",
    },
    true,
    ["sign", "verify"],
  );
  const exportedEc = await crypto.subtle.exportKey("jwk", ecKeyPair.publicKey);
  ecJwk = { ...exportedEc, kid: TEST_EC_KID, alg: "ES256", use: "sig" };
});

afterEach(() => {
  fetchMock.deactivate();
  vi.restoreAllMocks();
});

function base64UrlEncode(data) {
  const str = typeof data === "string" ? data : JSON.stringify(data);
  return btoa(str).replace(/=/g, "").replace(/\+/g, "-").replace(/\//g, "_");
}

async function createSignedJwt(
  payload,
  kid = TEST_KID,
  overrides = {},
  keyPair = testKeyPair,
  signAlgorithm = "RSASSA-PKCS1-v1_5",
) {
  const header = { alg: "RS256", typ: "JWT", kid, ...overrides };
  const headerB64 = base64UrlEncode(header);
  const payloadB64 = base64UrlEncode(payload);
  const data = new TextEncoder().encode(`${headerB64}.${payloadB64}`);
  const signature = await crypto.subtle.sign(
    signAlgorithm,
    keyPair.privateKey,
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
    repository: "aaif-goose/goose",
    ref: "refs/heads/main",
    sub: "repo:aaif-goose/goose:ref:refs/heads/main",
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

function mockOidc(keys = [testJwk]) {
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
    .reply(200, JSON.stringify({ keys }))
    .persist();
}

function mockOidcOnce(keys = [testJwk]) {
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
    );
  oidc
    .intercept({ path: "/.well-known/jwks", method: "GET" })
    .reply(200, JSON.stringify({ keys }));
}

function mockUpstream(upstreamStatus = 200, upstreamBody = { ok: true }) {
  fetchMock
    .get("https://api.anthropic.com")
    .intercept({ path: /.*/, method: "POST" })
    .reply(upstreamStatus, JSON.stringify(upstreamBody));
}

function mockOidcConfigFailure(status = 503) {
  fetchMock.activate();
  fetchMock.disableNetConnect();

  fetchMock
    .get("https://token.actions.githubusercontent.com")
    .intercept({ path: "/.well-known/openid-configuration", method: "GET" })
    .reply(status, JSON.stringify({ error: "unavailable" }));
}

function mockJwksFailure(status = 502) {
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
    );
  oidc
    .intercept({ path: "/.well-known/jwks", method: "GET" })
    .reply(status, JSON.stringify({ error: "bad gateway" }));
}

function mockAllWithUpstreamHeaders(
  upstreamStatus = 200,
  upstreamBody = { ok: true },
  headers = {},
) {
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
    .reply(upstreamStatus, JSON.stringify(upstreamBody), { headers });
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
    ALLOWED_REPOS: "aaif-goose/goose",
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
      [{ iss: undefined }, "Invalid issuer"],
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

  it("rejects expired tokens when max token age is not configured", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(
      validPayload({ iat: now - 10, exp: now - 1 }),
    );
    const request = new Request("https://proxy.example.com/v1/messages", {
      headers: { "x-api-key": token },
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(
      request,
      testEnv({ MAX_TOKEN_AGE_SECONDS: undefined }),
      ctx,
    );
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("rejects disallowed refs", async () => {
    const token = await createSignedJwt(
      validPayload({ ref: "refs/heads/feature" }),
    );
    const request = new Request("https://proxy.example.com/v1/messages", {
      headers: { "x-api-key": token },
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(
      request,
      testEnv({ ALLOWED_REFS: "refs/heads/main, refs/tags/v1" }),
      ctx,
    );
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe(
      "Ref 'refs/heads/feature' not allowed",
    );
  });

  it("reports OIDC discovery failures", async () => {
    const token = await createSignedJwt(validPayload());
    mockOidcConfigFailure(503);

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toContain(
      "Failed to fetch OIDC config: 503",
    );
  });

  it("reports JWKS fetch failures", async () => {
    const token = await createSignedJwt(validPayload());
    mockJwksFailure(502);

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toContain("Failed to fetch JWKS: 502");
  });

  it("verifies ES256 tokens", async () => {
    const token = await createSignedJwt(
      validPayload(),
      TEST_EC_KID,
      { alg: "ES256" },
      ecKeyPair,
      { name: "ECDSA", hash: "SHA-256" },
    );
    mockOidcOnce([ecJwk]);
    mockUpstream(200, { ok: true });

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

  it("refreshes JWKS and accepts a key found on retry", async () => {
    const token = await createSignedJwt(validPayload());
    mockOidcOnce([testJwk]);
    mockUpstream(200, { ok: true });

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

  it("rejects missing JWKS key after refresh", async () => {
    const token = await createSignedJwt(validPayload(), "missing-kid");
    mockOidc();

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("No matching key in JWKS");
  });

  it("rejects unsupported algorithms", async () => {
    const token = await createSignedJwt(validPayload(), TEST_KID, {
      alg: "HS256",
    });
    mockOidc();

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Unsupported algorithm: HS256");
  });

  it("rejects invalid signatures", async () => {
    const token = `${await createSignedJwt(validPayload())}tampered`;
    mockOidc();

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Invalid signature");
  });

  it("returns verification errors for undecodable JWT parts", async () => {
    const request = new Request("https://proxy.example.com/v1/messages", {
      headers: { "x-api-key": "bad.payload.signature" },
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(request, testEnv(), ctx);
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(401);
    expect((await response.json()).error).toContain("Verification error:");
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

  it("accepts allowed refs", async () => {
    const token = await createSignedJwt(validPayload());
    mockAll(200, { ok: true });

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(
      request,
      testEnv({ ALLOWED_REFS: "refs/heads/main, refs/tags/v1" }),
      ctx,
    );
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(200);
  });

  it("accepts unexpired tokens when max token age is not configured", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(
      validPayload({ iat: undefined, exp: now + 300 }),
    );
    mockAll(200, { ok: true });

    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(
      request,
      testEnv({
        MAX_TOKEN_AGE_SECONDS: undefined,
        OIDC_AUDIENCE: undefined,
        ALLOWED_REPOS: undefined,
      }),
      ctx,
    );
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(200);
  });

  it("handles CORS preflight with configured origin and extra headers", async () => {
    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "OPTIONS",
    });
    const response = await worker.fetch(
      request,
      testEnv({
        CORS_ORIGIN: "https://app.example.com",
        CORS_EXTRA_HEADERS: "anthropic-version",
      }),
    );

    expect(response.status).toBe(204);
    expect(response.headers.get("Access-Control-Allow-Origin")).toBe(
      "https://app.example.com",
    );
    expect(response.headers.get("Access-Control-Allow-Headers")).toBe(
      "Authorization, Content-Type, x-api-key, anthropic-version",
    );
  });

  it("handles CORS preflight with defaults", async () => {
    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "OPTIONS",
    });
    const response = await worker.fetch(request, testEnv());

    expect(response.status).toBe(204);
    expect(response.headers.get("Access-Control-Allow-Origin")).toBe("*");
    expect(response.headers.get("Access-Control-Allow-Headers")).toBe(
      "Authorization, Content-Type, x-api-key",
    );
  });

  it("accepts bearer auth, default upstream auth header, prefix, array audience, and fallback jti", async () => {
    const token = await createSignedJwt(
      validPayload({
        aud: ["other", "goose-oidc-proxy"],
        jti: undefined,
      }),
    );
    mockAllWithUpstreamHeaders(200, { ok: true }, {
      "Content-Encoding": "gzip",
      "Content-Length": "99",
    });

    const request = new Request("https://proxy.example.com/v1/messages?beta=1", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const bucketCalls = [];
    const response = await worker.fetch(
      request,
      testEnv({
        UPSTREAM_AUTH_HEADER: undefined,
        UPSTREAM_AUTH_PREFIX: "Bearer ",
        CORS_ORIGIN: "https://client.example.com",
        TOKEN_BUCKET: {
          idFromName: (jti) => {
            bucketCalls.push(jti);
            return "mock-id";
          },
          get: () => ({
            fetch: async () => Response.json({ allowed: true, remaining: 198 }),
          }),
        },
      }),
      ctx,
    );
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Encoding")).toBeNull();
    expect(response.headers.get("Content-Length")).toBeNull();
    expect(response.headers.get("Access-Control-Allow-Origin")).toBe(
      "https://client.example.com",
    );
    expect(bucketCalls[0]).toContain(
      "https://token.actions.githubusercontent.com:",
    );
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

describe("TokenBucket durable object", () => {
  function createState(storedCount) {
    const writes = [];
    return {
      writes,
      storage: {
        get: async () => storedCount,
        put: async (key, value) => writes.push([key, value]),
      },
    };
  }

  it("allows requests, stores counts, and does not reinitialize twice", async () => {
    const state = createState(undefined);
    const bucket = new TokenBucket({ storage: state.storage });

    const first = await bucket.fetch(
      new Request("https://bucket/check", {
        method: "POST",
        body: JSON.stringify({ maxRequests: 3, ratePerSecond: 3 }),
      }),
    );
    const second = await bucket.fetch(
      new Request("https://bucket/check", {
        method: "POST",
        body: JSON.stringify({ maxRequests: 3, ratePerSecond: 3 }),
      }),
    );

    expect(await first.json()).toEqual({ allowed: true, remaining: 2 });
    expect(await second.json()).toEqual({ allowed: true, remaining: 1 });
    expect(state.writes).toEqual([
      ["count", 1],
      ["count", 2],
    ]);
  });

  it("loads stored count and rejects exhausted budgets", async () => {
    const bucket = new TokenBucket({ storage: createState(5).storage });

    const response = await bucket.fetch(
      new Request("https://bucket/check", {
        method: "POST",
        body: JSON.stringify({ maxRequests: 5, ratePerSecond: 5 }),
      }),
    );

    expect(await response.json()).toEqual({
      allowed: false,
      error: "budget_exhausted",
    });
  });

  it("rate limits when recent timestamps reach limit", async () => {
    vi.spyOn(Date, "now").mockReturnValue(10_000);
    const bucket = new TokenBucket({ storage: createState(undefined).storage });

    await bucket.fetch(
      new Request("https://bucket/check", {
        method: "POST",
        body: JSON.stringify({ maxRequests: 5, ratePerSecond: 1 }),
      }),
    );
    const response = await bucket.fetch(
      new Request("https://bucket/check", {
        method: "POST",
        body: JSON.stringify({ maxRequests: 5, ratePerSecond: 1 }),
      }),
    );

    expect(await response.json()).toEqual({
      allowed: false,
      error: "rate_limited",
    });
    expect(response.headers.get("Retry-After")).toBe("1");
  });

  it("returns 404 for unknown paths", async () => {
    const bucket = new TokenBucket({ storage: createState(undefined).storage });

    const response = await bucket.fetch(new Request("https://bucket/missing"));

    expect(response.status).toBe(404);
    expect(await response.json()).toEqual({ error: "not found" });
  });

  it("uses default bucket limits when env omits them", async () => {
    const token = await createSignedJwt(validPayload());
    mockAll(200, { ok: true });

    const bodies = [];
    const request = new Request("https://proxy.example.com/v1/messages", {
      method: "POST",
      headers: { "x-api-key": token, "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    const ctx = createExecutionContext();
    const response = await worker.fetch(
      request,
      testEnv({
        MAX_REQUESTS_PER_TOKEN: undefined,
        RATE_LIMIT_PER_SECOND: undefined,
        TOKEN_BUCKET: {
          idFromName: () => "mock-id",
          get: () => ({
            fetch: async (_url, init) => {
              bodies.push(JSON.parse(init.body));
              return Response.json({ allowed: true, remaining: 199 });
            },
          }),
        },
      }),
      ctx,
    );
    await waitOnExecutionContext(ctx);

    expect(response.status).toBe(200);
    expect(bodies).toEqual([{ maxRequests: 200, ratePerSecond: 2 }]);
  });
});
