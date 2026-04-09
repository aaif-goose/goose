import { test as base, Page, Browser, chromium } from '@playwright/test';
import { exec, spawn, ChildProcess } from 'child_process';
import { join } from 'path';
import { promisify } from 'util';

const execAsync = promisify(exec);

type GooseTestFixtures = {
  goosePage: Page;
};

/**
 * Test-scoped fixture that launches a fresh Electron app for EACH test.
 *
 * Isolation: ⚠️ Partial - each test gets a fresh app instance, but uses ambient user config
 * Speed: ⚠️ Slow - ~3s startup overhead per test
 *
 * This ensures each test starts with a fresh app instance, but the app uses the
 * user's existing Goose configuration (providers, models, etc.).
 *
 * Usage:
 *   import { test, expect } from './fixtures';
 *
 *   test('my test', async ({ goosePage }) => {
 *     await goosePage.waitForSelector('[data-testid="chat-input"]');
 *     // ... test code
 *   });
 */
export const test = base.extend<GooseTestFixtures>({
  // Test-scoped fixture: launches a fresh Electron app for each test
  goosePage: async ({ browserName }, providePage, testInfo) => {
    void browserName;
    console.log(`Launching fresh Electron app for test: ${testInfo.title}`);

    let appProcess: ChildProcess | null = null;
    let browser: Browser | null = null;

    try {
      // Assign a unique debug port for this test to enable parallel execution
      // Base port 9222, offset by worker index * 100 + parallel slot
      const debugPort = 9222 + (testInfo.parallelIndex * 10);
      console.log(`Using debug port ${debugPort} for parallel test execution`);

      // Start the electron-forge process with Playwright remote debugging enabled
      // Use detached mode on Unix to create a process group we can kill together
      appProcess = spawn('pnpm', ['run', 'start-gui'], {
        cwd: join(__dirname, '../..'),
        stdio: 'pipe',
        detached: process.platform !== 'win32',
        env: {
          ...process.env,
          ELECTRON_IS_DEV: '1',
          NODE_ENV: 'development',
          GOOSE_ALLOWLIST_BYPASS: 'true',
          ENABLE_PLAYWRIGHT: 'true',
          PLAYWRIGHT_DEBUG_PORT: debugPort.toString(), // Unique port per test for parallel execution
          RUST_LOG: 'info', // Enable info-level logging for goosed backend
        }
      });

      // Log process output for debugging
      if (process.env.DEBUG_TESTS) {
        appProcess.stdout?.on('data', (data) => {
          console.log('App stdout:', data.toString());
        });

        appProcess.stderr?.on('data', (data) => {
          console.log('App stderr:', data.toString());
        });
      }

      // Wait for the app to start and remote debugging to be available
      // Retry connection until it succeeds (app is ready) or timeout
      console.log(`Waiting for Electron app to start on port ${debugPort}...`);
      const maxRetries = 100; // 100 retries * 100ms = 10 seconds max
      const retryDelay = 100; // 100ms between retries

      for (let attempt = 1; attempt <= maxRetries; attempt++) {
        try {
          browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
          console.log(`Connected to Electron app on attempt ${attempt} (~${(attempt * retryDelay) / 1000}s)`);
          break;
        } catch (error) {
          const errorMessage = error instanceof Error ? error.message : String(error);
          if (attempt === maxRetries) {
            throw new Error(`Failed to connect to Electron app after ${maxRetries} attempts (${(maxRetries * retryDelay) / 1000}s). Last error: ${errorMessage}`);
          }
          // Wait before next retry
          await new Promise(resolve => setTimeout(resolve, retryDelay));
        }
      }

      if (!browser) {
        throw new Error('Browser connection failed unexpectedly');
      }

      // Wait for Electron to create its first window after the CDP endpoint is up.
      let page: Page | null = null;
      for (let attempt = 1; attempt <= 100; attempt++) {
        const contexts = browser.contexts();
        page = contexts.flatMap((context) => context.pages())[0] ?? null;
        if (page) {
          break;
        }
        await new Promise((resolve) => setTimeout(resolve, 100));
      }

      if (!page) {
        throw new Error('No windows/pages found');
      }

      // Wait for page to be ready
      await page.waitForLoadState('domcontentloaded');

      // Try to wait for networkidle
      try {
        await page.waitForLoadState('networkidle', { timeout: 10000 });
      } catch {
        console.log('NetworkIdle timeout (likely due to MCP activity), continuing...');
      }

      // Wait for React app to be ready
      await page.waitForFunction(() => {
        const root = document.getElementById('root');
        return root && root.children.length > 0;
      }, { timeout: 30000 });

      console.log('App ready, starting test...');

      // Provide the page to the test
      await providePage(page);

    } finally {
      console.log('Cleaning up Electron app for this test...');

      // Close the CDP connection
      if (browser) {
        await browser.close().catch(console.error);
      }

      // Kill the npm process tree
      if (appProcess && appProcess.pid) {
        try {
          if (process.platform === 'win32') {
            // On Windows, kill the entire process tree
            await execAsync(`taskkill /F /T /PID ${appProcess.pid}`);
          } else {
            // On Unix, kill the entire process group
            try {
              // First try SIGTERM for graceful shutdown
              process.kill(-appProcess.pid, 'SIGTERM');
              await new Promise(resolve => setTimeout(resolve, 2000));
            } catch {
              // Process might already be dead
            }
            // Then SIGKILL if still running
            try {
              process.kill(-appProcess.pid, 'SIGKILL');
            } catch {
              // Process already exited
            }
          }
          console.log('Cleaned up app process');
        } catch (error) {
          if (
            error instanceof Error &&
            !('code' in error && error.code === 'ESRCH') &&
            !error.message.includes('No such process')
          ) {
            console.error('Error killing app process:', error);
          }
        }
      }
    }
  },
});

export { expect } from '@playwright/test';
