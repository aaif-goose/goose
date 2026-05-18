import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import path from 'node:path';
import { setupGoosed, type GoosedTestContext } from './setup';
import { readTypedConfig, patchTypedConfig, readConfig } from '../../src/api';

describe('typed config fresh state tests', () => {
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

  // --- Group I: Fresh config tests (no prior provider/state) ---

  it('fresh config has no active provider or providers block', async () => {
    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.response).toBeOkResponse();
    expect(getRes.data!.GOOSE_PROVIDER).toBeNull();
    expect(getRes.data!.active_provider).toBeNull();
    const providers = getRes.data!.providers;
    expect(providers === null || providers === undefined || Object.keys(providers).length === 0).toBe(true);
  });

  it('GOOSE_MODEL-only PATCH persists on fresh config (Fix 4 regression)', async () => {
    const patchRes = await patchTypedConfig({
      client: ctx.client,
      body: { GOOSE_MODEL: 'my-custom-model' },
    });
    expect(patchRes.response).toBeOkResponse();
    expect(patchRes.data!.GOOSE_MODEL).toBe('my-custom-model');

    const getRes = await readTypedConfig({ client: ctx.client });
    expect(getRes.data!.GOOSE_MODEL).toBe('my-custom-model');

    const legacyRes = await readConfig({
      client: ctx.client,
      body: { key: 'GOOSE_MODEL', is_secret: false },
    });
    expect(legacyRes.response).toBeOkResponse();
    expect(legacyRes.data).toBe('my-custom-model');
  });
});
