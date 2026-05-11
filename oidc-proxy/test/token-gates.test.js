// Comprehensive coverage for the dual-gate token validation defined in
// PLAN.md (issue #8832). Both `exp` (IdP) and MAX_TOKEN_AGE_SECONDS
// (operator) must independently pass; MAX_TOKEN_AGE_SECONDS is a stricter
// upper bound layered on top of `exp`, never a replacement.
//
// These tests are written from the spec in PLAN.md only — they assert
// expected behavior rather than codifying what the implementation does.
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
const TEST_KID = "test-kid-gates";

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
function payloadWith(overrides = {}) {
  const now = Math.floor(Date.now() / 1000);
  const base = {
    iss: "https://token.actions.githubusercontent.com",
    aud: "goose-oidc-proxy",
    iat: now - 10,
    exp: now + 300,
    jti: `gate-jti-${++jtiCounter}`,
    repository: "aaif-goose/goose",
    ref: "refs/heads/main",
    sub: "repo:aaif-goose/goose:ref:refs/heads/main",
    ...overrides,
  };
  // Allow callers to drop a claim by passing `undefined`
  for (const k of Object.keys(base)) {
    if (base[k] === undefined) delete base[k];
  }
  return base;
}

function mockOidcAndUpstream(upstreamStatus = 200, upstreamBody = { ok: true }) {
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

function mockTokenBucket(overrides = {}) {
  const defaults = { allowed: true, remaining: 199 };
  const response = { ...defaults, ...overrides };
  return {
    idFromName: () => "mock-id",
    get: () => ({ fetch: async () => Response.json(response) }),
  };
}

function gateEnv(overrides = {}) {
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

async function send(token, envObj) {
  const request = new Request("https://proxy.example.com/v1/messages", {
    method: "POST",
    headers: { "x-api-key": token, "Content-Type": "application/json" },
    body: JSON.stringify({}),
  });
  const ctx = createExecutionContext();
  const response = await worker.fetch(request, envObj, ctx);
  await waitOnExecutionContext(ctx);
  return response;
}

// ---------------------------------------------------------------------------
// Happy paths — both gates pass.
// ---------------------------------------------------------------------------
describe("dual-gate validation: happy paths", () => {
  it("recent iat + future exp + cap configured → 200", async () => {
    mockOidcAndUpstream(200, { id: "ok-1" });
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 10, exp: now + 300 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(200);
  });

  it("recent iat + future exp + cap unset → 200 (cap absent doesn't break valid tokens)", async () => {
    mockOidcAndUpstream(200, { id: "ok-2" });
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 10, exp: now + 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: undefined }));

    expect(response.status).toBe(200);
  });

  it("recent iat + future exp + cap empty string → 200 (empty truthiness skips age cap)", async () => {
    mockOidcAndUpstream(200, { id: "ok-3" });
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 10, exp: now + 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "" }));

    expect(response.status).toBe(200);
  });

  it("missing iat + future exp + cap configured → 200 (age cap silently skipped per `&& payload.iat` guard)", async () => {
    mockOidcAndUpstream(200, { id: "ok-4" });
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(
      payloadWith({ iat: undefined, exp: now + 300 }),
    );

    const response = await send(token, gateEnv());

    expect(response.status).toBe(200);
  });

  it("very large cap value tolerates an old (but unexpired) token → 200", async () => {
    mockOidcAndUpstream(200, { id: "ok-5" });
    const now = Math.floor(Date.now() / 1000);
    // iat is 10 minutes old, but cap is enormous; exp is still ahead.
    const token = await createSignedJwt(payloadWith({ iat: now - 600, exp: now + 60 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "999999999" }));

    expect(response.status).toBe(200);
  });
});

// ---------------------------------------------------------------------------
// `exp` is enforced unconditionally — the whole point of the fix.
// ---------------------------------------------------------------------------
describe("dual-gate validation: exp gate is unconditional", () => {
  it("expired token with cap configured → 401 'Token expired' (no bypass)", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 600, exp: now - 300 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("expired token with cap unset → 401 'Token expired'", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 600, exp: now - 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: undefined }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("expired token with cap empty string → 401 'Token expired'", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 600, exp: now - 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "" }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("missing exp + fresh iat + cap configured → 401 'Token expired' (was accepted pre-fix)", async () => {
    // PLAN §4: behavior change. Pre-fix this would have been accepted under
    // the age cap; post-fix exp is required.
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 10, exp: undefined }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("missing exp + cap unset → 401 'Token expired'", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 10, exp: undefined }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: undefined }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("missing both iat and exp + cap configured → 401 'Token expired'", async () => {
    const token = await createSignedJwt(
      payloadWith({ iat: undefined, exp: undefined }),
    );

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("exp = 0 (falsy) + cap configured → 401 'Token expired'", async () => {
    // The spec checks `!payload.exp`, so 0 is falsy and triggers expired.
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 10, exp: 0 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("exp 1 second in the past → 401 'Token expired' (no clock skew leeway per PLAN §4)", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 10, exp: now - 1 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });
});

// ---------------------------------------------------------------------------
// Age cap (MAX_TOKEN_AGE_SECONDS) gate — fires independently of exp.
// ---------------------------------------------------------------------------
describe("dual-gate validation: MAX_TOKEN_AGE_SECONDS gate", () => {
  it("iat past cap + exp still valid → 401 'Token too old' (cap fires independently of exp)", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 1500, exp: now + 300 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token too old");
  });

  it("iat very far past cap + exp far in future → 401 'Token too old'", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(
      payloadWith({ iat: now - 100_000, exp: now + 100_000 }),
    );

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token too old");
  });

  it("iat exactly at cap boundary → 200 (the check is age > cap, not >=)", async () => {
    mockOidcAndUpstream(200, { id: "boundary" });
    const now = Math.floor(Date.now() / 1000);
    // age == 1200 exactly should NOT be "too old" because the comparison is `>`.
    const token = await createSignedJwt(payloadWith({ iat: now - 1200, exp: now + 300 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(200);
  });

  it("iat 1 second past cap → 401 'Token too old' (boundary just outside)", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 1201, exp: now + 300 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token too old");
  });

  it("missing iat + valid exp + cap set → 200 (per `&& payload.iat` guard, cap silently skipped)", async () => {
    mockOidcAndUpstream(200, { id: "no-iat-but-valid" });
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: undefined, exp: now + 300 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(200);
  });

  it("very stale iat + cap unset → 200 (no cap means age is unconstrained)", async () => {
    mockOidcAndUpstream(200, { id: "no-cap" });
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(
      payloadWith({ iat: now - 100_000, exp: now + 300 }),
    );

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: undefined }));

    expect(response.status).toBe(200);
  });

  it("MAX_TOKEN_AGE_SECONDS='0' + positive age → 401 'Token too old' (parseInt('0',10)=0; any positive age fails)", async () => {
    // PLAN §4: '0' is a non-empty string → truthy → block runs; parseInt -> 0.
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 5, exp: now + 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "0" }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token too old");
  });

  it("very small cap '1' + iat 2 seconds old → 401 'Token too old'", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 2, exp: now + 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "1" }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token too old");
  });
});

// ---------------------------------------------------------------------------
// Both gates fail simultaneously — verifies ordering per PLAN §3 Step A.
// ---------------------------------------------------------------------------
describe("dual-gate validation: both gates fail simultaneously", () => {
  it("iat past cap AND exp expired → 401 'Token expired' (exp checked first per PLAN Step A)", async () => {
    // PLAN: "Order: `exp` first ... then the operator's age cap."
    // So even though the age cap also fails, the response should report exp.
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 1500, exp: now - 300 }));

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("missing exp AND iat past cap → 401 'Token expired' (exp gate wins)", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(
      payloadWith({ iat: now - 1500, exp: undefined }),
    );

    const response = await send(token, gateEnv());

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });
});

// ---------------------------------------------------------------------------
// Replay-attack scenario from the issue body — the canonical bug.
// ---------------------------------------------------------------------------
describe("dual-gate validation: issue #8832 replay scenario", () => {
  it("GitHub OIDC token (exp=iat+300) replayed past exp with cap=3600 → rejected", async () => {
    // PLAN §1 Concrete attack: token leaked at t=100s, replayed at t=400s
    // (past 300s exp), operator set cap=3600s.
    // Pre-fix: accepted. Post-fix: must reject as expired.
    const now = Math.floor(Date.now() / 1000);
    const issuedAt = now - 400; // we are now 400s past issuance
    const token = await createSignedJwt(
      payloadWith({ iat: issuedAt, exp: issuedAt + 300 }), // exp = now - 100
    );

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "3600" }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });
});

// ---------------------------------------------------------------------------
// Reason-string preservation — operators may have alerts grepping these.
// PLAN §4: "Keep `Token expired` and `Token too old` exactly as-is."
// ---------------------------------------------------------------------------
describe("dual-gate validation: reason strings are stable", () => {
  it("expired token reason is exactly 'Token expired' (no extra text)", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 600, exp: now - 300 }));

    const response = await send(token, gateEnv());
    const body = await response.json();

    expect(body.error).toBe("Token expired");
    expect(body.error).not.toMatch(/expir.*age|age.*expir/i);
  });

  it("too-old token reason is exactly 'Token too old' (no extra text)", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 1500, exp: now + 300 }));

    const response = await send(token, gateEnv());
    const body = await response.json();

    expect(body.error).toBe("Token too old");
  });

  it("response status is 401 for both expired and too-old (not 400, not 403)", async () => {
    const now = Math.floor(Date.now() / 1000);

    const expired = await createSignedJwt(payloadWith({ iat: now - 600, exp: now - 300 }));
    const tooOld = await createSignedJwt(payloadWith({ iat: now - 1500, exp: now + 300 }));

    const r1 = await send(expired, gateEnv());
    const r2 = await send(tooOld, gateEnv());

    expect(r1.status).toBe(401);
    expect(r2.status).toBe(401);
  });
});

// ---------------------------------------------------------------------------
// PLAN §5 test-strategy table — exact rows reproduced for traceability.
// ---------------------------------------------------------------------------
describe("dual-gate validation: PLAN §5 test-strategy matrix", () => {
  it("row 1: iat=-10, exp=+300, cap=1200 → 200 (forwards to upstream)", async () => {
    mockOidcAndUpstream(200, { id: "row-1" });
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 10, exp: now + 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "1200" }));

    expect(response.status).toBe(200);
    expect((await response.json()).id).toBe("row-1");
  });

  it("row 2: iat=-1500, exp=+300, cap=1200 → 401 'Token too old'", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 1500, exp: now + 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "1200" }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token too old");
  });

  it("row 3 (NEW, replaces buggy test): iat=-600, exp=-300, cap=1200 → 401 'Token expired'", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 600, exp: now - 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "1200" }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });

  it("row 4 (NEW regression): iat=-1500, exp=+300, cap=1200 → 401 'Token too old' (independent age gate)", async () => {
    // Distinct from row 2 in *intent*: asserts the age cap fires even when
    // exp is comfortably valid (independent of exp).
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 1500, exp: now + 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: "1200" }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token too old");
  });

  it("row 5 (NEW defensive): iat=-600, exp=-300, cap unset → 401 'Token expired'", async () => {
    const now = Math.floor(Date.now() / 1000);
    const token = await createSignedJwt(payloadWith({ iat: now - 600, exp: now - 300 }));

    const response = await send(token, gateEnv({ MAX_TOKEN_AGE_SECONDS: undefined }));

    expect(response.status).toBe(401);
    expect((await response.json()).error).toBe("Token expired");
  });
});
