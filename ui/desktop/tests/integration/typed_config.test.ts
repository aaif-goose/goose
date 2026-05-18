import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import path from 'node:path';
import { setupGoosed, type GoosedTestContext } from './setup';
import {
  readTypedConfig,
  patchTypedConfig,
  readConfig,
  upsertConfig,
  removeConfig,
  readAllConfig,
  getExtensions,
  addExtension,
} from '../../src/api';

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

  // --- Group H: Provider switching stress ---

  it('three-provider round-trip preserves each provider model', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'openai', GOOSE_MODEL: 'gpt-4o' },
    });
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'anthropic', GOOSE_MODEL: 'claude-sonnet-4-20250514' },
    });
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'ollama', GOOSE_MODEL: 'llama3.2' },
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
    expect(getRes.data!.providers!['openai'].model).toBe('gpt-4o');
    expect(getRes.data!.providers!['anthropic'].model).toBe('claude-sonnet-4-20250514');
    expect(getRes.data!.providers!['ollama'].model).toBe('llama3.2');
  });

  it('rapid sequential provider switches produce correct final state', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'openai', GOOSE_MODEL: 'gpt-4o-mini' },
    });
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'anthropic', GOOSE_MODEL: 'claude-haiku-3-5' },
    });
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'openai' },
    });

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_PROVIDER).toBe('openai');
    expect(getRes.data!.GOOSE_MODEL).toBe('gpt-4o-mini');
    expect(getRes.data!.providers!['anthropic'].model).toBe('claude-haiku-3-5');
  });

  it('switching to never-configured provider does not inherit current model', async () => {
    const beforeRes = await readTypedConfig({ client: ctx.client });
    const currentModel = beforeRes.data!.GOOSE_MODEL;
    expect(currentModel).toBeTruthy();

    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_PROVIDER: 'groq' },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_PROVIDER).toBe('groq');
    expect(patchRes.data!.GOOSE_MODEL).not.toBe(currentModel);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_PROVIDER).toBe('groq');
    expect(getRes.data!.GOOSE_MODEL).not.toBe(currentModel);
  });

  // --- Group J: Secret write verification ---

  it('PATCH secret and non-secret simultaneously — both persist', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_DEBUG: true, ANTHROPIC_API_KEY: 'sk-ant-test-j1' },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_DEBUG).toBe(true);
    const patchKeys = Object.keys(patchRes.data!);
    expect(patchKeys).not.toContain('ANTHROPIC_API_KEY');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_DEBUG).toBe(true);

    const secretRes = await readConfig({
      client: ctx.client,
      body: { key: 'ANTHROPIC_API_KEY', is_secret: true },
    });
    expect(secretRes.response).toBeOkResponse();
    expect(secretRes.data).toBeTruthy();
  });

  it('PATCH multiple secrets simultaneously — all persist', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { OPENAI_API_KEY: 'sk-test-j2', GOOGLE_API_KEY: 'goog-test-j2' },
    });
    expect(patchRes.response).toBeOkResponse();

    const openaiRes = await readConfig({
      client: ctx.client,
      body: { key: 'OPENAI_API_KEY', is_secret: true },
    });
    expect(openaiRes.response).toBeOkResponse();
    expect(openaiRes.data).toBeTruthy();

    const googleRes = await readConfig({
      client: ctx.client,
      body: { key: 'GOOGLE_API_KEY', is_secret: true },
    });
    expect(googleRes.response).toBeOkResponse();
    expect(googleRes.data).toBeTruthy();
  });

  // --- Group K: Legacy/typed cross-endpoint interop ---

  it('write via typed PATCH, read via legacy readConfig', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_ALLOWLIST: 'test-allowlist-k1', GOOSE_STATUS_HOOK: '/tmp/hook.sh' },
    });

    const allowlistRes = await readConfig({
      client: ctx.client,
      body: { key: 'GOOSE_ALLOWLIST', is_secret: false },
    });
    expect(allowlistRes.response).toBeOkResponse();
    expect(allowlistRes.data).toBe('test-allowlist-k1');

    const hookRes = await readConfig({
      client: ctx.client,
      body: { key: 'GOOSE_STATUS_HOOK', is_secret: false },
    });
    expect(hookRes.response).toBeOkResponse();
    expect(hookRes.data).toBe('/tmp/hook.sh');
  });

  it('write via legacy upsertConfig, read via typed GET', async () => {
    const upsertRes = await upsertConfig({
      client: ctx.client,
      body: { key: 'GOOSE_STREAM_TIMEOUT', value: 120, is_secret: false },
    });
    expect(upsertRes.response).toBeOkResponse();

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_STREAM_TIMEOUT).toBe(120);
  });

  it('readAllConfig and readTypedConfig agree on shared keys', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_DEBUG: true, GOOSE_MAX_TOKENS: 2048 },
    });

    const allRes = await readAllConfig({ client: ctx.client });
    expect(allRes.response).toBeOkResponse();

    const typedRes = await readTypedConfig({ client: ctx.client });
    expect(typedRes.response).toBeOkResponse();

    const allConfig = (allRes.data as { config: Record<string, unknown> }).config;
    expect(allConfig['GOOSE_DEBUG']).toBe(true);
    expect(typedRes.data!.GOOSE_DEBUG).toBe(true);
    expect(allConfig['GOOSE_MAX_TOKENS']).toBe(2048);
    expect(typedRes.data!.GOOSE_MAX_TOKENS).toBe(2048);

    const typedKeys = Object.keys(typedRes.data!);
    for (const secret of SECRET_FIELDS) {
      expect(typedKeys).not.toContain(secret);
    }
  });

  // --- Group L: Extensions cross-endpoint ---

  it('extension written via typed PATCH is visible via GET /config/extensions', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: {
        extensions: {
          'myext-l1': {
            enabled: true,
            type: 'sse',
            uri: 'http://localhost:9999/sse',
            name: 'myext-l1',
            description: 'test ext',
          },
        },
      },
    });

    const extRes = await getExtensions({ client: ctx.client });
    expect(extRes.response).toBeOkResponse();
    const extNames = extRes.data!.extensions.map((e: { name?: string }) => e.name);
    expect(extNames).toContain('myext-l1');
  });

  it('extension added via POST /config/extensions is visible via typed GET', async () => {
    const addRes = await addExtension({
      client: ctx.client,
      body: {
        name: 'myext-l2',
        config: {
          type: 'sse',
          uri: 'http://localhost:9998/sse',
          name: 'myext-l2',
          description: 'test l2',
        },
        enabled: true,
      },
    });
    expect(addRes.response).toBeOkResponse();

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.extensions!['myext-l2']).toBeDefined();
    expect(getRes.data!.extensions!['myext-l2'].enabled).toBe(true);
  });

  // --- Group M: Providers block ---

  it('PATCH providers with two entries round-trips correctly', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: {
        providers: {
          openai: { enabled: true, model: 'gpt-4o', configured: true },
          anthropic: { enabled: true, model: 'claude-opus-4', configured: true },
        },
      },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.providers!['openai'].model).toBe('gpt-4o');
    expect(patchRes.data!.providers!['anthropic'].model).toBe('claude-opus-4');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.providers!['openai'].model).toBe('gpt-4o');
    expect(getRes.data!.providers!['anthropic'].model).toBe('claude-opus-4');
  });

  it('providers block uses whole-value replacement, not deep merge', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: {
        providers: {
          openai: { enabled: true, model: 'gpt-4o', configured: true },
          groq: { enabled: false, model: 'llama3', configured: false },
        },
      },
    });

    const midGet = await readTypedConfig({ client: ctx.client });
    expect(midGet.data!.providers!['groq']).toBeDefined();

    await patchTypedConfig({
      client: ctx.client,
      body: {
        providers: {
          anthropic: { enabled: true, model: 'claude-haiku-3-5', configured: true },
        },
      },
    });

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.providers!['anthropic'].model).toBe('claude-haiku-3-5');
    expect(getRes.data!.providers!['openai']).toBeUndefined();
    expect(getRes.data!.providers!['groq']).toBeUndefined();
  });

  it('active_provider field round-trips via typed PATCH', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: {
        providers: {
          openai: { enabled: true, model: 'gpt-4o', configured: true },
        },
        active_provider: 'openai',
      },
    });

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.active_provider).toBe('openai');
  });

  it('PATCH providers block and GOOSE_PROVIDER in same request', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: {
        providers: {
          openai: { enabled: true, model: 'gpt-4o', configured: true },
          anthropic: { enabled: true, model: 'claude-sonnet-4-20250514', configured: true },
        },
        GOOSE_PROVIDER: 'openai',
      },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_PROVIDER).toBe('openai');
    expect(patchRes.data!.GOOSE_MODEL).toBe('gpt-4o');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_PROVIDER).toBe('openai');
    expect(getRes.data!.GOOSE_MODEL).toBe('gpt-4o');
    expect(getRes.data!.providers!['anthropic'].model).toBe('claude-sonnet-4-20250514');
  });

  // --- Group N: removeConfig interaction ---

  it('set via typed PATCH, remove via legacy, verify via typed GET', async () => {
    await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_ALLOWLIST: 'allowlist-to-remove' },
    });

    const midGet = await readTypedConfig({ client: ctx.client });
    expect(midGet.data!.GOOSE_ALLOWLIST).toBe('allowlist-to-remove');

    const removeRes = await removeConfig({
      client: ctx.client,
      body: { key: 'GOOSE_ALLOWLIST', is_secret: false },
    });
    expect(removeRes.response).toBeOkResponse();

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_ALLOWLIST).toBeNull();
  });

  it('remove non-existent key does not error', async () => {
    const removeRes = await removeConfig({
      client: ctx.client,
      body: { key: 'GOOSE_PROMPT_EDITOR', is_secret: false },
    });
    expect(removeRes.response.status).toBeLessThan(500);
  });

  // --- Group O: Multi-field and miscellaneous ---

  it('PATCH all primitive types simultaneously', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: {
        GOOSE_PROVIDER: 'openai',
        GOOSE_DEBUG: false,
        GOOSE_MAX_TOKENS: 16384,
        SECURITY_PROMPT_THRESHOLD: 0.5,
        GOOSE_SEARCH_PATHS: ['/tmp/x', '/tmp/y'],
        GOOSE_MODE: 'chat',
        ANTHROPIC_THINKING_BUDGET: 3000,
      },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_PROVIDER).toBe('openai');
    expect(patchRes.data!.GOOSE_DEBUG).toBe(false);
    expect(patchRes.data!.GOOSE_MAX_TOKENS).toBe(16384);
    expect(patchRes.data!.SECURITY_PROMPT_THRESHOLD).toBe(0.5);
    expect(patchRes.data!.GOOSE_SEARCH_PATHS).toEqual(['/tmp/x', '/tmp/y']);
    expect(patchRes.data!.GOOSE_MODE).toBe('chat');
    expect(patchRes.data!.ANTHROPIC_THINKING_BUDGET).toBe(3000);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_PROVIDER).toBe('openai');
    expect(getRes.data!.GOOSE_DEBUG).toBe(false);
    expect(getRes.data!.GOOSE_MAX_TOKENS).toBe(16384);
    expect(getRes.data!.SECURITY_PROMPT_THRESHOLD).toBe(0.5);
    expect(getRes.data!.GOOSE_SEARCH_PATHS).toEqual(['/tmp/x', '/tmp/y']);
    expect(getRes.data!.GOOSE_MODE).toBe('chat');
    expect(getRes.data!.ANTHROPIC_THINKING_BUDGET).toBe(3000);
  });

  it('slash_commands round-trip', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: {
        slash_commands: [
          { command: '/test-cmd', recipe_path: '/tmp/test.yaml' },
          { command: '/another', recipe_path: '/tmp/another.yaml' },
        ],
      },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.slash_commands).toHaveLength(2);
    expect(patchRes.data!.slash_commands![0].command).toBe('/test-cmd');
    expect(patchRes.data!.slash_commands![0].recipe_path).toBe('/tmp/test.yaml');
    expect(patchRes.data!.slash_commands![1].command).toBe('/another');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.slash_commands).toHaveLength(2);
    expect(getRes.data!.slash_commands![0].command).toBe('/test-cmd');
  });

  it('mixed nested objects and scalars in same PATCH request', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: {
        GOOSE_MODE: 'auto',
        experiments: { 'exp-alpha': true },
        extensions: {
          'testextO3': {
            enabled: true,
            type: 'builtin',
            name: 'testextO3',
            description: 'test',
          },
        },
      },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_MODE).toBe('auto');
    expect(patchRes.data!.experiments).toEqual({ 'exp-alpha': true });
    expect(patchRes.data!.extensions!['testextO3']).toBeDefined();
    expect(patchRes.data!.extensions!['testextO3'].enabled).toBe(true);

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_MODE).toBe('auto');
    expect(getRes.data!.experiments).toEqual({ 'exp-alpha': true });
    expect(getRes.data!.extensions!['testextO3'].enabled).toBe(true);
  });
});
