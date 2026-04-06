export class TokenBucket {
  constructor(state) {
    this.state = state;
    this.count = 0;
    this.timestamps = [];
    this.initialized = false;
  }

  async initialize() {
    if (this.initialized) return;
    const stored = await this.state.storage.get("count");
    if (stored !== undefined) this.count = stored;
    this.initialized = true;
  }

  async fetch(request) {
    await this.initialize();

    const url = new URL(request.url);
    if (url.pathname === "/check") {
      const { maxRequests, ratePerSecond } = await request.json();

      if (this.count >= maxRequests) {
        return Response.json({ allowed: false, error: "budget_exhausted" });
      }

      const now = Date.now();
      this.timestamps = this.timestamps.filter((t) => now - t < 1000);
      if (this.timestamps.length >= ratePerSecond) {
        return Response.json(
          { allowed: false, error: "rate_limited" },
          { headers: { "Retry-After": "1" } },
        );
      }

      this.count++;
      this.timestamps.push(now);
      await this.state.storage.put("count", this.count);

      return Response.json({
        allowed: true,
        remaining: maxRequests - this.count,
      });
    }

    return Response.json({ error: "not found" }, { status: 404 });
  }
}

export default {
  async fetch(request, env) {
    if (request.method === "OPTIONS") {
      return handleCors(env);
    }

    const token =
      request.headers.get("x-api-key") ||
      request.headers.get("Authorization")?.replace("Bearer ", "");
    if (!token) {
      return jsonResponse(401, { error: "Missing authentication" });
    }

    const result = await verifyOidcToken(token, env);
    if (!result.valid) {
      return jsonResponse(401, { error: result.reason });
    }

    // Check rate limit and budget via Durable Object
    const bucketCheck = await checkTokenBucket(result.jti, env);
    if (!bucketCheck.allowed) {
      if (bucketCheck.error === "rate_limited") {
        return jsonResponse(
          429,
          { error: "Rate limit exceeded" },
          { "Retry-After": "1" },
        );
      }
      return jsonResponse(429, { error: "Token budget exhausted" });
    }

    const url = new URL(request.url);
    const upstreamUrl = `${env.UPSTREAM_URL}${url.pathname}${url.search}`;

    const headers = new Headers(request.headers);
    headers.delete("Authorization");
    headers.delete("x-api-key");

    const authHeader = env.UPSTREAM_AUTH_HEADER || "Authorization";
    const authPrefix = env.UPSTREAM_AUTH_PREFIX;
    headers.set(
      authHeader,
      authPrefix
        ? `${authPrefix}${env.UPSTREAM_API_KEY}`
        : env.UPSTREAM_API_KEY,
    );
    headers.set("Host", new URL(env.UPSTREAM_URL).host);

    const response = await fetch(upstreamUrl, {
      method: request.method,
      headers,
      body: request.body,
    });

    const respHeaders = new Headers(response.headers);
    // Workers' fetch auto-decompresses but keeps the Content-Encoding header,
    // which would cause clients to try decompressing already-decompressed data.
    respHeaders.delete("Content-Encoding");
    respHeaders.delete("Content-Length");
    respHeaders.set("Access-Control-Allow-Origin", env.CORS_ORIGIN || "*");

    return new Response(response.body, {
      status: response.status,
      headers: respHeaders,
    });
  },
};

// --- Token bucket (Durable Object) ---

async function checkTokenBucket(jti, env) {
  const maxRequests = parseInt(env.MAX_REQUESTS_PER_TOKEN || "200", 10);
  const ratePerSecond = parseInt(env.RATE_LIMIT_PER_SECOND || "2", 10);

  const id = env.TOKEN_BUCKET.idFromName(jti);
  const stub = env.TOKEN_BUCKET.get(id);

  const resp = await stub.fetch("https://bucket/check", {
    method: "POST",
    body: JSON.stringify({ maxRequests, ratePerSecond }),
  });

  return resp.json();
}

// --- OIDC JWT verification using Web Crypto API ---

let jwksCache = null;
let jwksCacheTime = 0;
const JWKS_CACHE_TTL_MS = 60 * 60 * 1000; // 1 hour

async function fetchJwks(issuer) {
  const now = Date.now();
  if (jwksCache && now - jwksCacheTime < JWKS_CACHE_TTL_MS) {
    return jwksCache;
  }

  const wellKnownUrl = `${issuer.replace(/\/$/, "")}/.well-known/openid-configuration`;
  const configResp = await fetch(wellKnownUrl);
  if (!configResp.ok) {
    throw new Error(`Failed to fetch OIDC config: ${configResp.status}`);
  }
  const config = await configResp.json();

  const jwksResp = await fetch(config.jwks_uri);
  if (!jwksResp.ok) {
    throw new Error(`Failed to fetch JWKS: ${jwksResp.status}`);
  }

  jwksCache = await jwksResp.json();
  jwksCacheTime = now;
  return jwksCache;
}

function base64UrlDecode(str) {
  const padded = str.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(padded);
  return Uint8Array.from(binary, (c) => c.charCodeAt(0));
}

function decodeJwtPart(b64url) {
  return JSON.parse(new TextDecoder().decode(base64UrlDecode(b64url)));
}

const ALG_MAP = {
  RS256: { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
  RS384: { name: "RSASSA-PKCS1-v1_5", hash: "SHA-384" },
  RS512: { name: "RSASSA-PKCS1-v1_5", hash: "SHA-512" },
  ES256: { name: "ECDSA", namedCurve: "P-256", hash: "SHA-256" },
  ES384: { name: "ECDSA", namedCurve: "P-384", hash: "SHA-384" },
};

async function verifyOidcToken(token, env) {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) {
      return { valid: false, reason: "Malformed JWT" };
    }

    const [headerB64, payloadB64, sigB64] = parts;
    const header = decodeJwtPart(headerB64);
    const payload = decodeJwtPart(payloadB64);

    if (env.MAX_TOKEN_AGE_SECONDS && payload.iat) {
      const age = Date.now() / 1000 - payload.iat;
      if (age > parseInt(env.MAX_TOKEN_AGE_SECONDS, 10)) {
        return { valid: false, reason: "Token too old" };
      }
    } else if (!payload.exp || payload.exp < Date.now() / 1000) {
      return { valid: false, reason: "Token expired" };
    }

    const expectedIssuer = env.OIDC_ISSUER.replace(/\/$/, "");
    const actualIssuer = (payload.iss || "").replace(/\/$/, "");
    if (actualIssuer !== expectedIssuer) {
      return { valid: false, reason: "Invalid issuer" };
    }

    if (env.OIDC_AUDIENCE) {
      const audiences = Array.isArray(payload.aud)
        ? payload.aud
        : [payload.aud];
      if (!audiences.includes(env.OIDC_AUDIENCE)) {
        return { valid: false, reason: "Invalid audience" };
      }
    }

    if (env.ALLOWED_REPOS) {
      const allowed = env.ALLOWED_REPOS.split(",").map((r) => r.trim());
      if (!allowed.includes(payload.repository)) {
        return {
          valid: false,
          reason: `Repository '${payload.repository}' not allowed`,
        };
      }
    }

    if (env.ALLOWED_REFS) {
      const allowed = env.ALLOWED_REFS.split(",").map((r) => r.trim());
      if (!allowed.includes(payload.ref)) {
        return { valid: false, reason: `Ref '${payload.ref}' not allowed` };
      }
    }

    const jwks = await fetchJwks(env.OIDC_ISSUER);
    let jwk = jwks.keys.find((k) => k.kid === header.kid);
    if (!jwk) {
      jwksCache = null;
      const refreshed = await fetchJwks(env.OIDC_ISSUER);
      jwk = refreshed.keys.find((k) => k.kid === header.kid);
      if (!jwk) {
        return { valid: false, reason: "No matching key in JWKS" };
      }
    }

    const sigResult = await verifySignature(
      header,
      jwk,
      headerB64,
      payloadB64,
      sigB64,
    );
    if (!sigResult.valid) return sigResult;

    const jti = payload.jti || `${payload.iss}:${payload.iat}:${payload.sub}`;
    return { valid: true, jti };
  } catch (err) {
    return { valid: false, reason: `Verification error: ${err.message}` };
  }
}

async function verifySignature(header, jwk, headerB64, payloadB64, sigB64) {
  const alg = ALG_MAP[header.alg];
  if (!alg) {
    return { valid: false, reason: `Unsupported algorithm: ${header.alg}` };
  }

  const keyAlgorithm = alg.namedCurve
    ? { name: alg.name, namedCurve: alg.namedCurve }
    : { name: alg.name, hash: alg.hash };

  const cryptoKey = await crypto.subtle.importKey(
    "jwk",
    jwk,
    keyAlgorithm,
    false,
    ["verify"],
  );

  const data = new TextEncoder().encode(`${headerB64}.${payloadB64}`);
  const signature = base64UrlDecode(sigB64);

  const verifyAlgorithm =
    alg.name === "ECDSA" ? { name: alg.name, hash: alg.hash } : alg.name;

  const valid = await crypto.subtle.verify(
    verifyAlgorithm,
    cryptoKey,
    signature,
    data,
  );
  if (!valid) {
    return { valid: false, reason: "Invalid signature" };
  }

  return { valid: true };
}

// --- Helpers ---

function jsonResponse(status, body, extraHeaders = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", ...extraHeaders },
  });
}

function handleCors(env) {
  const extraHeaders = env.CORS_EXTRA_HEADERS || "";
  const baseHeaders = "Authorization, Content-Type, x-api-key";
  const allowHeaders = extraHeaders
    ? `${baseHeaders}, ${extraHeaders}`
    : baseHeaders;

  return new Response(null, {
    status: 204,
    headers: {
      "Access-Control-Allow-Origin": env.CORS_ORIGIN || "*",
      "Access-Control-Allow-Methods": "GET, POST, PUT, PATCH, DELETE, OPTIONS",
      "Access-Control-Allow-Headers": allowHeaders,
      "Access-Control-Max-Age": "86400",
    },
  });
}
