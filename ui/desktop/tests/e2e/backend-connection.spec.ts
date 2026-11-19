import { _electron, expect, test, type ElectronApplication, type Page } from '@playwright/test';
import { execFile } from 'node:child_process';
import { createHash, X509Certificate } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:https';
import { isAbsolute, join, resolve } from 'node:path';
import { promisify } from 'node:util';

const run = promisify(execFile);
const desktopDir = resolve(__dirname, '../..');
const renderer = '.vite/renderer/main_window/index.html';

async function buildTestApp(appDir: string, profileDir: string) {
  const { build } = await import('vite');
  await mkdir(join(appDir, '.vite/renderer/main_window'), { recursive: true });
  await writeFile(join(appDir, renderer), '<!doctype html><title>Backend connection test</title>');
  await writeFile(
    join(appDir, 'package.json'),
    JSON.stringify({ main: 'entry.cjs', version: '1.0.0' })
  );
  await writeFile(
    join(appDir, 'entry.cjs'),
    `
    const { app, WebContentsView } = require('electron');
    app.setPath('userData', ${JSON.stringify(profileDir)});
    app.whenReady().then(async () => {
      const view = new WebContentsView({ webPreferences: { partition: 'persist:goose' } });
      await view.webContents.loadFile(${JSON.stringify(join(appDir, renderer))});
      await view.webContents.executeJavaScript(${JSON.stringify(`
        localStorage.setItem('theme', 'dark');
        localStorage.setItem('use_system_theme', 'false');
        localStorage.setItem('response_style', 'detailed');
        localStorage.setItem('show_pricing', 'false');
        localStorage.setItem('seenAnnouncementIds', '["previous-announcement"]');
        localStorage.setItem('navigation_expanded', 'false');
        localStorage.setItem('navigation_width', '264');
        localStorage.setItem('sessions_include_acp', 'true');
        localStorage.setItem('goose-chat-history', JSON.stringify([{content:'previous input',timestamp:Date.now()}]));
      `)});
      view.webContents.close();
      require('./.vite/build/main.js');
    }).catch((error) => { console.error(error); app.exit(1); });
  `
  );
  for (const entry of ['main', 'preload']) {
    await build({
      configFile: false,
      define: {
        MAIN_WINDOW_VITE_DEV_SERVER_URL: 'undefined',
        MAIN_WINDOW_VITE_NAME: JSON.stringify('main_window'),
      },
      logLevel: 'error',
      build: {
        outDir: join(appDir, '.vite/build'),
        emptyOutDir: false,
        lib: {
          entry: join(desktopDir, `src/${entry}.ts`),
          formats: ['cjs'],
          fileName: () => `${entry}.js`,
        },
        rollupOptions: {
          external: (id) => !isAbsolute(id) && /^[a-z@]/i.test(id),
        },
      },
    });
  }
}

async function startBackend(fixtureDir: string, secret: string) {
  await mkdir(fixtureDir, { recursive: true });
  const certPath = join(fixtureDir, 'cert.pem');
  const keyPath = join(fixtureDir, 'key.pem');
  await run('openssl', [
    'req',
    '-x509',
    '-newkey',
    'rsa:2048',
    '-nodes',
    '-days',
    '1',
    '-subj',
    '/CN=127.0.0.1',
    '-keyout',
    keyPath,
    '-out',
    certPath,
  ]);
  const [cert, key] = await Promise.all([readFile(certPath), readFile(keyPath)]);
  const authenticatedProbes: string[] = [];
  const socketTokens: string[] = [];
  let statusRequests = 0;
  const server = createServer({ cert, key }, (request, response) => {
    if (request.url === '/status') {
      statusRequests += 1;
      response.writeHead(200).end();
      return;
    }
    const received = request.headers['x-secret-key'];
    if (typeof received === 'string') authenticatedProbes.push(received);
    response.writeHead(received === secret ? 406 : 401).end();
  });
  server.on('upgrade', (request, socket) => {
    const token = new URL(request.url ?? '', 'https://127.0.0.1').searchParams.get('token');
    if (token !== secret) {
      socket.end('HTTP/1.1 401 Unauthorized\r\n\r\n');
      return;
    }
    socketTokens.push(token);
    const accept = createHash('sha1')
      .update(`${request.headers['sec-websocket-key']}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
      .digest('base64');
    socket.write(
      `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`
    );
    socket.on('data', () => socket.end(Buffer.from([0x88, 0x00])));
  });
  await new Promise<void>((ready) => server.listen(0, '127.0.0.1', ready));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Expected a TCP backend address');
  return {
    config: {
      enabled: true,
      url: `https://127.0.0.1:${address.port}`,
      certFingerprint: new X509Certificate(cert).fingerprint256,
      secret,
      workingDir: fixtureDir,
    },
    authenticatedProbes,
    socketTokens,
    get statusRequests() {
      return statusRequests;
    },
    close: () => new Promise<void>((done) => server.close(() => done())),
  };
}

async function connectWebSocket(page: Page) {
  await page.evaluate(async () => {
    const url = await window.electron.getAcpUrl();
    if (!url) throw new Error('The window has no ACP lease');
    await new Promise<void>((done, fail) => {
      const socket = new WebSocket(url);
      socket.onopen = () => {
        socket.close();
        done();
      };
      socket.onerror = () => fail(new Error('Could not open the ACP WebSocket'));
    });
  });
}

function rendererPolicy(page: Page): Promise<string | null> {
  return page.evaluate(
    () =>
      new Promise<string | null>((done) => {
        window.addEventListener('securitypolicyviolation', (event) => done(event.originalPolicy), {
          once: true,
        });
        void fetch('https://csp-check.invalid/').catch(() => undefined);
        setTimeout(() => done(null), 1000);
      })
  );
}

async function firstChatWindow(electron: ElectronApplication): Promise<Page> {
  const findChat = () => electron.windows().find((page) => page.url().includes('#/'));
  await expect.poll(() => Boolean(findChat())).toBe(true);
  return findChat()!;
}

test('two backend windows keep distinct pins and CSP while sharing migrated preferences', async ({}, testInfo) => {
  const appDir = testInfo.outputPath('app');
  const profileDir = testInfo.outputPath('profile');
  const first = await startBackend(testInfo.outputPath('first-backend'), 'first-test-key');
  const second = await startBackend(testInfo.outputPath('second-backend'), 'second-test-key');
  let electron: ElectronApplication | undefined;
  try {
    await mkdir(profileDir, { recursive: true });
    await writeFile(
      join(profileDir, 'settings.json'),
      JSON.stringify({
        showMenuBarIcon: false,
        externalGoosed: first.config,
      })
    );
    await buildTestApp(appDir, profileDir);
    electron = await _electron.launch({
      args: [appDir],
      env: { ...process.env, GOOSE_EXTERNAL_BACKEND: '', GOOSE_PATH_ROOT: profileDir },
    });
    const firstPage = await firstChatWindow(electron);
    await expect(firstPage).toHaveTitle('Backend connection test');
    await connectWebSocket(firstPage);
    const secondWindow = electron.waitForEvent('window');
    await firstPage.evaluate(async (config) => {
      await window.electron.setSetting('externalGoosed', config);
      window.electron.createChatWindow();
    }, second.config);
    const secondPage = await secondWindow;
    await expect(secondPage).toHaveTitle('Backend connection test');
    await connectWebSocket(secondPage);
    await firstPage.reload();
    await connectWebSocket(firstPage);

    expect(first.authenticatedProbes).toEqual([first.config.secret]);
    expect(second.authenticatedProbes).toEqual([second.config.secret]);
    expect(first.socketTokens).toEqual([first.config.secret, first.config.secret]);
    expect(second.socketTokens).toEqual([second.config.secret]);
    const crossPinRequestsSucceeded = await electron.evaluate(
      async ({ BrowserWindow }, origins) => {
        const windows = BrowserWindow.getAllWindows();
        if (new Set(windows.map((win) => win.webContents.session)).size !== 2) {
          throw new Error('Each backend window must have its own Electron session');
        }
        return Promise.all(
          windows.map(async ({ webContents }) => {
            const ownUrl: string = await webContents.executeJavaScript(
              'window.electron.getAcpUrl()'
            );
            const otherOrigin = origins.find(
              (origin) => new URL(origin).host !== new URL(ownUrl).host
            );
            try {
              await webContents.session.fetch(`${otherOrigin}/status`);
              return true;
            } catch {
              return false;
            }
          })
        );
      },
      [first.config.url, second.config.url]
    );
    expect(crossPinRequestsSucceeded).toEqual([false, false]);
    expect([first.statusRequests, second.statusRequests]).toEqual([1, 1]);

    for (const [page, backend, other] of [
      [firstPage, first, second],
      [secondPage, second, first],
    ] as const) {
      const policy = await rendererPolicy(page);
      const connectSources = policy
        ?.split(';')
        .find((part) => part.startsWith('connect-src '))
        ?.split(' ');
      expect(connectSources).toContain(backend.config.url.replace('https:', 'wss:'));
      expect(connectSources).not.toContain(other.config.url.replace('https:', 'wss:'));
      expect(policy).toContain('upgrade-insecure-requests');
      expect(
        await page.evaluate(async () => ({
          theme: window.appConfig.get('GOOSE_THEME'),
          useSystemTheme: window.appConfig.get('GOOSE_USE_SYSTEM_THEME'),
          style: await window.electron.getSetting('responseStyle'),
          showPricing: await window.electron.getSetting('showPricing'),
          announcements: await window.electron.getSetting('seenAnnouncementIds'),
          expanded: window.electron.getRendererStorage('navigation_expanded'),
          width: window.electron.getRendererStorage('navigation_width'),
          includeAcp: window.electron.getRendererStorage('sessions_include_acp'),
          history: JSON.parse(window.electron.getRendererStorage('goose-chat-history') ?? '[]'),
        }))
      ).toEqual({
        theme: 'dark',
        useSystemTheme: false,
        style: 'detailed',
        showPricing: false,
        announcements: ['previous-announcement'],
        expanded: 'false',
        width: '264',
        includeAcp: 'true',
        history: [{ content: 'previous input', timestamp: expect.any(Number) }],
      });
    }

    await secondPage.evaluate(() => window.electron.appendMessageHistory('new input'));
    expect(
      await firstPage.evaluate(() =>
        JSON.parse(window.electron.getRendererStorage('goose-chat-history') ?? '[]').map(
          ({ content }: { content: string }) => content
        )
      )
    ).toEqual(['previous input', 'new input']);
    const settings = JSON.parse(await readFile(join(profileDir, 'settings.json'), 'utf8'));
    expect(settings.rendererStorage).not.toHaveProperty('goose-chat-history');
    expect(JSON.parse(await readFile(join(profileDir, 'chat-input-history.json'), 'utf8'))).toEqual([
      { content: 'previous input', timestamp: expect.any(Number) },
      { content: 'new input', timestamp: expect.any(Number) },
    ]);

    const standaloneWindow = electron.waitForEvent('window');
    await firstPage.evaluate(() =>
      window.electron.launchApp({
        name: 'Connection test',
        uri: 'ui://connection-test/index.html',
        mimeType: 'text/html',
      })
    );
    const standalonePage = await standaloneWindow;
    await expect(standalonePage).toHaveURL(/#\/standalone-app\?/);
    await connectWebSocket(standalonePage);
    expect(first.socketTokens).toHaveLength(3);
    expect(await rendererPolicy(standalonePage)).toBe(await rendererPolicy(firstPage));
    expect(
      await electron.evaluate(({ BrowserWindow }) => {
        const windows = BrowserWindow.getAllWindows();
        return new Set(windows.map((window) => window.webContents.session)).size;
      })
    ).toBe(2);
  } finally {
    await electron?.close();
    await Promise.all([first.close(), second.close()]);
  }
});

test('moves input history out of settings for profiles that already migrated', async ({}, testInfo) => {
  const appDir = testInfo.outputPath('app');
  const profileDir = testInfo.outputPath('profile');
  const settingsPath = join(profileDir, 'settings.json');
  const backend = await startBackend(testInfo.outputPath('backend'), 'test-key');
  let electron: ElectronApplication | undefined;
  try {
    await mkdir(profileDir, { recursive: true });
    const history = [{ content: 'saved input', timestamp: Date.now() }];
    await writeFile(
      settingsPath,
      JSON.stringify({
        showMenuBarIcon: false,
        externalGoosed: backend.config,
        rendererStorageMigrated: true,
        rendererStorage: { navigation_width: '264', 'goose-chat-history': JSON.stringify(history) },
      })
    );
    await buildTestApp(appDir, profileDir);
    electron = await _electron.launch({
      args: [appDir],
      env: { ...process.env, GOOSE_EXTERNAL_BACKEND: '', GOOSE_PATH_ROOT: profileDir },
    });
    const page = await firstChatWindow(electron);
    await expect(page).toHaveTitle('Backend connection test');
    expect(await page.evaluate(() => window.electron.getRendererStorage('goose-chat-history'))).toBe(
      JSON.stringify(history)
    );
    expect(JSON.parse(await readFile(settingsPath, 'utf8')).rendererStorage).toEqual({
      navigation_width: '264',
    });
    await page.evaluate(() => window.electron.setRendererStorage('goose-chat-history', null));
    expect(await readFile(join(profileDir, 'chat-input-history.json'), 'utf8')).toBe('[]');
  } finally {
    await electron?.close();
    await backend.close();
  }
});
