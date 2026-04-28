/**
 * Provider smoke tests — normal mode (direct tool calls).
 *
 * Each available provider/model pair gets its own test that spawns `goose run`
 * with the developer builtin, asks the model to read files via the shell tool,
 * and validates the output.
 */

import { expect, beforeAll } from 'vitest';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { buildGoose, discoverTestCases, runGoose, providerTest } from './test_providers_lib';

const BUILTINS = 'developer';
const TEST_CONTENT = 'test-content-abc123';

let gooseBin: string;
let testFile: string;

beforeAll(() => {
  gooseBin = buildGoose();

  const targetDir = path.resolve(process.cwd(), '..', '..', 'target');
  fs.mkdirSync(targetDir, { recursive: true });
  testFile = path.join(targetDir, 'test-content.txt');
  fs.writeFileSync(testFile, TEST_CONTENT + '\n');
});

const { testAgentic, testNonAgentic } = providerTest(discoverTestCases());

testNonAgentic('reads files via shell tool', async (tc) => {
  const testdir = fs.mkdtempSync(path.join(os.tmpdir(), 'goose-test-'));
  try {
    const tokenA = `smoke-alpha-${Math.floor(Math.random() * 32768)}`;
    const tokenB = `smoke-bravo-${Math.floor(Math.random() * 32768)}`;
    fs.writeFileSync(path.join(testdir, 'part-a.txt'), tokenA + '\n');
    fs.writeFileSync(path.join(testdir, 'part-b.txt'), tokenB + '\n');

    const output = await runGoose(
      gooseBin,
      testdir,
      'Use the shell tool to cat ./part-a.txt and ./part-b.txt, then reply with ONLY the contents of both files, one per line, nothing else.',
      BUILTINS,
      { GOOSE_PROVIDER: tc.provider, GOOSE_MODEL: tc.model }
    );

    const shellToolPattern = /(shell \| developer)|(▸.*shell)/;
    expect(
      shellToolPattern.test(output),
      `Expected model to use shell tool\n\nFull output:\n${output}`
    ).toBe(true);
    expect(
      output,
      `Expected output to contain token from part-a.txt (${tokenA})\n\nFull output:\n${output}`
    ).toContain(tokenA);
    expect(
      output,
      `Expected output to contain token from part-b.txt (${tokenB})\n\nFull output:\n${output}`
    ).toContain(tokenB);
  } finally {
    fs.rmSync(testdir, { recursive: true, force: true });
  }
});

testAgentic('reads file contents', async (tc) => {
  const testdir = fs.mkdtempSync(path.join(os.tmpdir(), 'goose-test-'));
  try {
    fs.copyFileSync(testFile, path.join(testdir, 'test-content.txt'));

    const output = await runGoose(
      gooseBin,
      testdir,
      'read ./test-content.txt and output its contents exactly',
      BUILTINS,
      { GOOSE_PROVIDER: tc.provider, GOOSE_MODEL: tc.model }
    );

    expect(
      output.toLowerCase(),
      `Expected model output to contain "${TEST_CONTENT}"\n\nFull output:\n${output}`
    ).toContain(TEST_CONTENT.toLowerCase());
  } finally {
    fs.rmSync(testdir, { recursive: true, force: true });
  }
});
