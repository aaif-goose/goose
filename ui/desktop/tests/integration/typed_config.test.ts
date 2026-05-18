import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import path from 'node:path';
import { setupGoosed, type GoosedTestContext } from './setup';
import { readTypedConfig, patchTypedConfig } from '../../src/api';

const SECRET_FIELDS = [
  'ANTHROPIC_API_KEY',
  'AVIAN_API_KEY',
  'AZURE_OPENAI_API_KEY',
  'DATABRICKS_TOKEN',
  'GOOGLE_API_KEY',
  'GROQ_API_KEY',
  'LITELLM_API_KEY',
  'LITELLM_CUSTOM_HEADERS',
  'NANOGPT_API_KEY',
  'OPENAI_API_KEY',
  'OPENROUTER_API_KEY',
  'SNOWFLAKE_TOKEN',
  'TETRATE_API_KEY',
  'VENICE_API_KEY',
  'XAI_API_KEY',
];

describe('typed config HTTP endpoints', () => {
  let ctx: GoosedTestContext;

  beforeAll(async () => {
    process.env.GOOSED_BINARY = path.resolve(__dirname, '..', '..', '..', '..', 'target', 'debug', 'goosed');
    ctx = await setupGoosed({
      configYaml: 'GOOSE_DISABLE_KEYRING: true\n',
    });
  }, 120_000);

  afterAll(async () => {
    await ctx.cleanup();
  });

  // --- Group A: GET baseline ---

  it('GET /config/typed returns 200 with a GooseConfigSchema object', async () => {
    const response = await readTypedConfig({ client: ctx.client });
    expect(response.response).toBeOkResponse();
    expect(response.data).toBeDefined();
    expect(typeof response.data).toBe('object');
  });

  it('GET /config/typed response has no secret fields', async () => {
    const response = await readTypedConfig({ client: ctx.client });
    expect(response.response).toBeOkResponse();
    const keys = Object.keys(response.data!);
    for (const secret of SECRET_FIELDS) {
      expect(keys).not.toContain(secret);
    }
  });

  // --- Group B: PATCH single-field updates by type ---

  it('PATCH string field: GOOSE_PROVIDER', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'openai' },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_PROVIDER).toBe('openai');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_PROVIDER).toBe('openai');
  });

  it('PATCH boolean field: GOOSE_DEBUG', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_DEBUG: true },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_DEBUG).toBe(true);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_DEBUG).toBe(true);
  });

  it('PATCH integer field: GOOSE_MAX_TOKENS', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_MAX_TOKENS: 8192 },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_MAX_TOKENS).toBe(8192);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_MAX_TOKENS).toBe(8192);
  });

  it('PATCH enum field: GOOSE_MODE', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_MODE: 'approve' },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_MODE).toBe('approve');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_MODE).toBe('approve');
  });

  it('PATCH float field: SECURITY_PROMPT_THRESHOLD', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { SECURITY_PROMPT_THRESHOLD: 0.85 },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.SECURITY_PROMPT_THRESHOLD).toBe(0.85);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.SECURITY_PROMPT_THRESHOLD).toBe(0.85);
  });

  it('PATCH array field: GOOSE_SEARCH_PATHS', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_SEARCH_PATHS: ['/tmp/a', '/tmp/b'] },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_SEARCH_PATHS).toEqual(['/tmp/a', '/tmp/b']);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_SEARCH_PATHS).toEqual(['/tmp/a', '/tmp/b']);
  });

  // --- Group C: PATCH sparse semantics ---

  it('omitted fields stay unchanged after PATCH', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_DEBUG: true },
    });

    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_MAX_TOKENS: 4096 },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_DEBUG).toBe(true);
    expect(patchRes.data!.GOOSE_MAX_TOKENS).toBe(4096);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_DEBUG).toBe(true);
    expect(getRes.data!.GOOSE_MAX_TOKENS).toBe(4096);
  });

  it('empty PATCH body is a no-op', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_DEBUG: false },
    });

    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: {},
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_DEBUG).toBe(false);
  });

  it('null does not clear a previously set value', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'anthropic' },
    });

    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: null },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_PROVIDER).toBe('anthropic');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_PROVIDER).toBe('anthropic');
  });

  // --- Group D: Nested object whole-value replacement ---

  it('extensions field uses whole-value replacement, not deep merge', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: {
        extensions: {
          'custom-ext-a': {
            enabled: true,
            type: 'builtin',
            name: 'custom-ext-a',
            description: 'First custom extension',
          },
        },
      },
    });

    const midGet = await readTypedConfig({ client: ctx.client });
    expect(midGet.data!.extensions!['custom-ext-a']).toBeDefined();

    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: {
        extensions: {
          'custom-ext-b': {
            enabled: false,
            type: 'builtin',
            name: 'custom-ext-b',
            description: 'Second custom extension',
          },
        },
      },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.extensions).toBeDefined();
    expect(patchRes.data!.extensions!['custom-ext-b']).toBeDefined();
    expect(patchRes.data!.extensions!['custom-ext-a']).toBeUndefined();

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.extensions!['custom-ext-b']).toBeDefined();
    expect(getRes.data!.extensions!['custom-ext-a']).toBeUndefined();
  });

  it('experiments field uses whole-value replacement', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { experiments: { 'feature-a': true } },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.experiments).toEqual({ 'feature-a': true });

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.experiments).toEqual({ 'feature-a': true });
  });

  // --- Group E: Secret field routing ---

  it('PATCH with secret API key returns 200 and does not leak secret in response', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { OPENAI_API_KEY: 'sk-test-sentinel-value' },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data).toBeDefined();

    const keys = Object.keys(patchRes.data!);
    for (const secret of SECRET_FIELDS) {
      expect(keys).not.toContain(secret);
    }
  });

  // --- Group F: Regression tests for review findings ---

  it('switching GOOSE_PROVIDER preserves the target provider model (not current)', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'openai', GOOSE_MODEL: 'gpt-4o' },
    });
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'anthropic', GOOSE_MODEL: 'claude-sonnet-4-20250514' },
    });

    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'openai' },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_PROVIDER).toBe('openai');
    expect(patchRes.data!.GOOSE_MODEL).toBe('gpt-4o');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_PROVIDER).toBe('openai');
    expect(getRes.data!.GOOSE_MODEL).toBe('gpt-4o');
  });

  it('PATCH ANTHROPIC_THINKING_BUDGET round-trips correctly', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { ANTHROPIC_THINKING_BUDGET: 5000 },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.ANTHROPIC_THINKING_BUDGET).toBe(5000);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.ANTHROPIC_THINKING_BUDGET).toBe(5000);
  });

  // --- Group G: Response shape validation ---

  it('PATCH response does not contain GooseConfigUpdate-only fields', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_DEBUG: true },
    });
    expect(patchRes.response).toBeOkResponse();

    const keys = Object.keys(patchRes.data!);
    for (const secret of SECRET_FIELDS) {
      expect(keys).not.toContain(secret);
    }
  });
});
