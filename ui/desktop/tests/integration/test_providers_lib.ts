/**
 * Shared library for provider smoke tests.
 *
 * Ported from scripts/test_providers_lib.sh — keeps the same provider config,
 * allowed-failure list, agentic-provider list, and environment detection.
 */

import { test } from 'vitest';
import { execSync, spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

// ---------------------------------------------------------------------------
// Provider configuration
// ---------------------------------------------------------------------------

type ModelEntry = string | { name: string; flaky: true };

interface ProviderConfig {
  provider: string;
  models: ModelEntry[];
  agentic?: boolean;
  available: () => boolean;
}

function modelName(entry: ModelEntry): string {
  return typeof entry === 'string' ? entry : entry.name;
}

function modelFlaky(entry: ModelEntry): boolean {
  return typeof entry !== 'string' && entry.flaky;
}

function hasEnv(name: string): boolean {
  return !!process.env[name];
}

function hasCmd(name: string): boolean {
  try {
    execSync(`command -v ${name}`, { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

function hasFile(p: string): boolean {
  return fs.existsSync(p);
}

function getProviders(): ProviderConfig[] {
  return [
    {
      provider: 'openrouter',
      models: [
        'google/gemini-2.5-pro',
        'anthropic/claude-sonnet-4.5',
        { name: 'qwen/qwen3-coder:exacto', flaky: true },
        'z-ai/glm-4.6:exacto',
        { name: 'nvidia/nemotron-3-nano-30b-a3b:free', flaky: true },
      ],
      available: () => hasEnv('OPENROUTER_API_KEY'),
    },
    {
      provider: 'xai',
      models: ['grok-3'],
      available: () => hasEnv('XAI_API_KEY'),
    },
    {
      provider: 'openai',
      models: [
        'gpt-4o',
        'gpt-4o-mini',
        { name: 'gpt-3.5-turbo', flaky: true },
        { name: 'gpt-5', flaky: true },
      ],
      available: () => hasEnv('OPENAI_API_KEY'),
    },
    {
      provider: 'anthropic',
      models: ['claude-sonnet-4-5-20250929', 'claude-opus-4-5-20251101'],
      available: () => hasEnv('ANTHROPIC_API_KEY'),
    },
    {
      provider: 'google',
      models: [
        'gemini-2.5-pro',
        { name: 'gemini-2.5-flash', flaky: true },
        { name: 'gemini-3-pro-preview', flaky: true },
        'gemini-3-flash-preview',
      ],
      available: () => hasEnv('GOOGLE_API_KEY'),
    },
    {
      provider: 'tetrate',
      models: ['claude-sonnet-4-20250514'],
      available: () => hasEnv('TETRATE_API_KEY'),
    },
    {
      provider: 'databricks',
      models: ['databricks-claude-sonnet-4', 'gemini-2-5-flash', 'gpt-4o'],
      available: () => hasEnv('DATABRICKS_HOST') && hasEnv('DATABRICKS_TOKEN'),
    },
    {
      provider: 'azure_openai',
      models: [process.env.AZURE_OPENAI_DEPLOYMENT_NAME ?? ''],
      available: () => hasEnv('AZURE_OPENAI_ENDPOINT') && hasEnv('AZURE_OPENAI_DEPLOYMENT_NAME'),
    },
    {
      provider: 'aws_bedrock',
      models: ['us.anthropic.claude-sonnet-4-5-20250929-v1:0'],
      available: () =>
        hasEnv('AWS_REGION') && (hasEnv('AWS_PROFILE') || hasEnv('AWS_ACCESS_KEY_ID')),
    },
    {
      provider: 'gcp_vertex_ai',
      models: ['gemini-2.5-pro'],
      available: () => hasEnv('GCP_PROJECT_ID'),
    },
    {
      provider: 'snowflake',
      models: ['claude-sonnet-4-5'],
      available: () => hasEnv('SNOWFLAKE_HOST') && hasEnv('SNOWFLAKE_TOKEN'),
    },
    {
      provider: 'venice',
      models: ['llama-3.3-70b'],
      available: () => hasEnv('VENICE_API_KEY'),
    },
    {
      provider: 'litellm',
      models: ['gpt-4o-mini'],
      available: () => hasEnv('LITELLM_API_KEY'),
    },
    {
      provider: 'sagemaker_tgi',
      models: ['sagemaker-tgi-endpoint'],
      available: () => hasEnv('SAGEMAKER_ENDPOINT_NAME') && hasEnv('AWS_REGION'),
    },
    {
      provider: 'github_copilot',
      models: ['gpt-4.1'],
      available: () =>
        hasEnv('GITHUB_COPILOT_TOKEN') ||
        hasFile(path.join(os.homedir(), '.config/goose/github_copilot_token.json')),
    },
    {
      provider: 'chatgpt_codex',
      models: ['gpt-5.4'],
      available: () =>
        hasEnv('CHATGPT_CODEX_TOKEN') ||
        hasFile(path.join(os.homedir(), '.config/goose/chatgpt_codex/tokens.json')),
    },
    {
      provider: 'claude-code',
      models: ['default'],
      agentic: true,
      available: () => hasCmd('claude'),
    },
    {
      provider: 'cursor-agent',
      models: ['auto'],
      agentic: true,
      available: () => hasCmd('cursor-agent'),
    },
    {
      provider: 'ollama',
      models: ['qwen3'],
      available: () => hasEnv('OLLAMA_HOST') || hasCmd('ollama'),
    },
  ];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function stripQuotes(s: string): string {
  if (
    s.length >= 2 &&
    ((s.startsWith('"') && s.endsWith('"')) || (s.startsWith("'") && s.endsWith("'")))
  ) {
    return s.slice(1, -1);
  }
  return s;
}

function loadDotenv(): void {
  // Resolve .env from the repository root (two levels up from ui/desktop).
  const repoRoot = path.resolve(__dirname, '..', '..', '..', '..');
  const envPath = path.join(repoRoot, '.env');
  if (!fs.existsSync(envPath)) return;
  const lines = fs.readFileSync(envPath, 'utf-8').split('\n');
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eqIdx = trimmed.indexOf('=');
    if (eqIdx === -1) continue;
    const key = trimmed.slice(0, eqIdx);
    const value = stripQuotes(trimmed.slice(eqIdx + 1));
    if (!(key in process.env)) {
      process.env[key] = value;
    }
  }
}

function shouldSkipProvider(provider: string): boolean {
  const skip = process.env.SKIP_PROVIDERS;
  if (!skip) return false;
  return skip
    .split(',')
    .map((s) => s.trim())
    .includes(provider);
}

// ---------------------------------------------------------------------------
// Build goose binary
// ---------------------------------------------------------------------------

export function buildGoose(): string {
  if (!process.env.SKIP_BUILD) {
    console.error('Building goose...');
    execSync('cargo build --bin goose', { stdio: 'inherit' });
    console.error('');
  } else {
    console.error('Skipping build (SKIP_BUILD is set)...');
    console.error('');
  }
  return path.resolve(process.cwd(), '..', '..', 'target/debug/goose');
}

// ---------------------------------------------------------------------------
// Test case discovery
// ---------------------------------------------------------------------------

export interface TestCase {
  provider: string;
  model: string;
  available: boolean;
  flaky: boolean;
  agentic: boolean;
  skippedReason?: string;
}

export function discoverTestCases(options?: { skipAgentic?: boolean }): TestCase[] {
  loadDotenv();
  const skipAgentic = options?.skipAgentic ?? false;
  const providers = getProviders();

  const testCases: TestCase[] = [];

  for (const pc of providers) {
    const providerAvailable = pc.available();
    const agentic = pc.agentic ?? false;

    for (const entry of pc.models) {
      const model = modelName(entry);
      const flaky = modelFlaky(entry);

      if (!providerAvailable) {
        testCases.push({
          provider: pc.provider,
          model,
          available: false,
          flaky,
          agentic,
          skippedReason: 'prerequisites not met',
        });
      } else if (shouldSkipProvider(pc.provider)) {
        testCases.push({
          provider: pc.provider,
          model,
          available: false,
          flaky,
          agentic,
          skippedReason: 'SKIP_PROVIDERS',
        });
      } else if (skipAgentic && agentic) {
        testCases.push({
          provider: pc.provider,
          model,
          available: false,
          flaky,
          agentic,
          skippedReason: 'agentic provider skipped in this mode',
        });
      } else {
        testCases.push({
          provider: pc.provider,
          model,
          available: true,
          flaky,
          agentic,
        });
      }
    }
  }

  return testCases;
}

// ---------------------------------------------------------------------------
// Test registration helpers
// ---------------------------------------------------------------------------

type ProviderTestFn = (tc: TestCase) => Promise<void>;

function registerTests(label: string, cases: TestCase[], fn: ProviderTestFn): void {
  const available = cases.filter((tc) => tc.available && !tc.flaky);
  const flaky = cases.filter((tc) => tc.available && tc.flaky);
  const skipped = cases.filter((tc) => !tc.available);

  if (available.length > 0) {
    test.each(available)(`${label} — $provider / $model`, async (tc) => {
      await fn(tc);
    });
  }

  if (flaky.length > 0) {
    // Use a longer vitest timeout (90s) so the internal runGoose timeout (55s)
    // fires first — that rejection is catchable and the test passes as "allowed".
    test.each(flaky)(
      `${label} — $provider / $model (flaky)`,
      async (tc) => {
        try {
          await fn(tc);
        } catch (err) {
          console.warn(`Flaky test ${tc.provider}/${tc.model} failed (allowed): ${err}`);
        }
      },
      90_000
    );
  }

  if (skipped.length > 0) {
    test.skip.each(skipped)(`${label} — $provider / $model — $skippedReason`, () => {});
  }
}

/**
 * Build decorator-style test registrars from a set of discovered test cases.
 *
 * Usage:
 *   const { testAll, testAgentic, testNonAgentic } = providerTest(cases);
 *
 *   testAll('reads a file', async (tc) => { ... });
 *   testAgentic('delegates work', async (tc) => { ... });
 *   testNonAgentic('uses shell tool', async (tc) => { ... });
 */
export function providerTest(cases: TestCase[]) {
  const agentic = cases.filter((tc) => tc.agentic);
  const nonAgentic = cases.filter((tc) => !tc.agentic);

  return {
    testAll: (label: string, fn: ProviderTestFn) => registerTests(label, cases, fn),
    testAgentic: (label: string, fn: ProviderTestFn) => registerTests(label, agentic, fn),
    testNonAgentic: (label: string, fn: ProviderTestFn) => registerTests(label, nonAgentic, fn),
  };
}

// ---------------------------------------------------------------------------
// Utility: run goose binary and capture output
// ---------------------------------------------------------------------------

export function runGoose(
  gooseBin: string,
  cwd: string,
  prompt: string,
  builtins: string,
  env: Record<string, string>,
  timeoutMs: number = 55_000
): Promise<string> {
  return new Promise((resolve, reject) => {
    const child: ChildProcess = spawn(
      gooseBin,
      ['run', '--text', prompt, '--with-builtin', builtins],
      {
        cwd,
        env: { ...process.env, ...env },
        stdio: ['ignore', 'pipe', 'pipe'],
      }
    );

    let output = '';
    let settled = false;

    const timer = setTimeout(() => {
      if (!settled) {
        settled = true;
        child.kill('SIGKILL');
        reject(new Error(`goose timed out after ${timeoutMs}ms\n\nPartial output:\n${output}`));
      }
    }, timeoutMs);

    child.stdout?.on('data', (d) => {
      output += String(d);
    });
    child.stderr?.on('data', (d) => {
      output += String(d);
    });

    child.on('close', () => {
      if (!settled) {
        settled = true;
        clearTimeout(timer);
        resolve(output);
      }
    });

    child.on('error', (err) => {
      if (!settled) {
        settled = true;
        clearTimeout(timer);
        resolve(`spawn error: ${err.message}`);
      }
    });
  });
}
