import { _electron, expect, test, type TestInfo, type WebSocketRoute } from '@playwright/test';
import { once } from 'node:events';
import { mkdir, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { isAbsolute, join, resolve } from 'node:path';
import type { AddressInfo } from 'node:net';
import { build } from 'vite';

const desktopDir = resolve(__dirname, '../..');
const recipe = {
  title: 'Deep link recipe',
  description: 'Test recipe',
  prompt: 'Write about {{ topic }}.',
};
const requestParamsMethod = '_goose/unstable/session/recipe/request-params';

type RequestId = string | number;
interface AcpRequest {
  id?: RequestId;
  method?: string;
  params?: {
    sessionId?: string;
    cwd?: string;
    workingDir?: string;
    deeplink?: string;
    _meta?: Record<string, unknown>;
    prompt?: Array<{ text?: string }>;
  };
  result?: { action: string; values?: Record<string, string> };
}

function createAcpPeer() {
  const launches: NonNullable<AcpRequest['params']>[] = [];
  const decodedLinks: string[] = [];
  const prompts: Array<{ sessionId: string; text: string }> = [];
  const cancelledRuns: string[] = [];
  let finishOriginalRun: (() => void) | undefined;
  const sessions = new Map<string, { cwd: string; recipe?: typeof recipe }>();

  function connect(socket: WebSocketRoute) {
    const queuedCallbacks = new Map<string, () => void>();
    const pending = new Map<
      RequestId,
      {
        requestId: RequestId;
        sessionId: string;
      }
    >();
    const send = (message: object) => socket.send(JSON.stringify({ jsonrpc: '2.0', ...message }));
    const reply = (id: RequestId, result: object) => send({ id, result });

    socket.onMessage((data) => {
      const message = JSON.parse(data.toString()) as AcpRequest;
      const { id, method, params, result } = message;
      if (method === 'session/cancel' && params?.sessionId) {
        cancelledRuns.push(params.sessionId);
      }
      if (id === undefined) return;

      const pendingRecipe = pending.get(id);
      if (pendingRecipe) {
        pending.delete(id);
        const { requestId, sessionId } = pendingRecipe;
        if (result?.action !== 'submit') {
          send({
            id: requestId,
            error: {
              code: -32000,
              message: 'Recipe parameters cancelled',
              data: { reason: 'recipe_params_cancelled' },
            },
          });
          return;
        }
        const session = sessions.get(sessionId)!;
        session.recipe = { ...recipe, prompt: `Write about ${result.values?.topic}.` };
        reply(requestId, { sessionId, _meta: { recipe: session.recipe } });
        return;
      }

      switch (method) {
        case 'initialize':
          reply(id, {
            protocolVersion: 1,
            agentCapabilities: { _meta: { goose: { recipeParameterScopes: true } } },
          });
          break;
        case '_goose/unstable/defaults/read':
          reply(id, { providerId: 'test-provider', modelId: 'test-model' });
          break;
        case '_goose/unstable/config/read-all':
          reply(id, { config: {} });
          break;
        case '_goose/unstable/config/extensions/list':
          reply(id, { extensions: [] });
          break;
        case 'session/list':
          reply(id, { sessions: [] });
          break;
        case '_goose/unstable/recipes/decode':
          decodedLinks.push(params!.deeplink!);
          reply(id, { recipe });
          break;
        case '_goose/unstable/recipes/scan':
          reply(id, { has_security_warnings: false });
          break;
        case 'session/new': {
          const launch = params!;
          const deeplink = launch._meta?.recipeDeeplink as string | undefined;
          const sessionId = deeplink ? `${deeplink}-session` : 'original';
          sessions.set(sessionId, { cwd: launch.cwd! });
          if (!deeplink) {
            reply(id, { sessionId });
            break;
          }
          launches.push(launch);
          const callbackId = `parameters-${sessionId}`;
          pending.set(callbackId, { requestId: id, sessionId });
          const requestParameters = () =>
            send({
              id: callbackId,
              method: requestParamsMethod,
              params: {
                sessionId,
                parameterScopeId: launch._meta?.recipeParameterScopeId,
                parameters: [
                  {
                    key: 'topic',
                    description: 'Topic',
                    input_type: 'string',
                    requirement: 'user_prompt',
                  },
                ],
              },
            });
          // The consent lookups can finish in either order. Issue callbacks with both
          // launches in flight so the form assertions exercise overlapping scopes.
          if (deeplink === 'queued' || deeplink === 'parameterless') {
            queuedCallbacks.set(deeplink, requestParameters);
            if (queuedCallbacks.size === 2) {
              queuedCallbacks.get('queued')!();
              queuedCallbacks.get('parameterless')!();
              queuedCallbacks.clear();
            }
          } else {
            requestParameters();
          }
          break;
        }
        case '_goose/unstable/session/info': {
          const sessionId = params!.sessionId!;
          const session = sessions.get(sessionId)!;
          reply(id, {
            session: {
              sessionId,
              cwd: session.cwd,
              title: session.recipe?.title ?? 'Original chat',
              _meta: { providerId: 'test-provider', modelId: 'test-model' },
            },
          });
          break;
        }
        case '_goose/unstable/session/working-dir/update':
          sessions.get(params!.sessionId!)!.cwd = params!.workingDir!;
          reply(id, {});
          break;
        case 'session/prompt': {
          const sessionId = params!.sessionId!;
          prompts.push({ sessionId, text: params!.prompt!.map((part) => part.text).join('') });
          if (sessionId !== 'original') {
            reply(id, { stopReason: 'end_turn' });
            break;
          }
          finishOriginalRun = () => {
            send({
              method: 'session/update',
              params: {
                sessionId,
                update: {
                  sessionUpdate: 'agent_message_chunk',
                  content: { type: 'text', text: 'Original run finished' },
                },
              },
            });
            reply(id, { stopReason: 'end_turn' });
          };
          break;
        }
        default:
          send({ id, error: { code: -32601, message: `Unhandled test ACP method: ${method}` } });
      }
    });
  }

  return {
    connect,
    launches,
    decodedLinks,
    prompts,
    cancelledRuns,
    finishOriginalRun: () => finishOriginalRun!(),
  };
}

async function launchDesktop(testInfo: TestInfo, startupLink?: string) {
  const appDir = testInfo.outputPath('desktop');
  const profile = testInfo.outputPath('profile');
  await mkdir(profile, { recursive: true });
  await build({
    configFile: join(desktopDir, 'vite.renderer.config.mts'),
    root: desktopDir,
    base: './',
    logLevel: 'error',
    build: { outDir: join(appDir, '.vite/renderer/main_window') },
  });
  for (const entry of ['main', 'preload']) {
    await build({
      configFile: false,
      logLevel: 'error',
      define: {
        MAIN_WINDOW_VITE_DEV_SERVER_URL: 'undefined',
        MAIN_WINDOW_VITE_NAME: JSON.stringify('main_window'),
      },
      build: {
        outDir: join(appDir, '.vite/build'),
        emptyOutDir: false,
        lib: {
          entry: join(desktopDir, `src/${entry}.ts`),
          formats: ['cjs'],
          fileName: () => `${entry}.js`,
        },
        rollupOptions: { external: (id) => !isAbsolute(id) && /^[a-z@]/i.test(id) },
      },
    });
  }
  await writeFile(
    join(appDir, 'package.json'),
    JSON.stringify({ main: 'entry.cjs', version: '1.0.0' })
  );
  await writeFile(
    join(appDir, 'entry.cjs'),
    `
    const { app } = require('electron');
    app.setPath('userData', ${JSON.stringify(profile)});
    app.setAsDefaultProtocolClient = () => true;
    app.once('start-recipe-test', (url) => {
      require('./.vite/build/main.js');
      if (url) {
        if (process.platform === 'darwin') app.emit('open-url', {}, url);
        else app.emit('second-instance', {}, [url]);
      }
    });
  `
  );

  const server = createServer((request, response) => {
    response.writeHead(request.url === '/status' ? 200 : 406).end();
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const { port } = server.address() as AddressInfo;
  let electron: Awaited<ReturnType<typeof _electron.launch>> | undefined;
  const close = async () => {
    try {
      await electron?.close();
    } finally {
      server.close();
    }
  };
  try {
    await writeFile(
      join(profile, 'settings.json'),
      JSON.stringify({
        showMenuBarIcon: false,
        externalGoosed: { enabled: true, url: `http://127.0.0.1:${port}`, secret: 'test-key' },
      })
    );
    electron = await _electron.launch({
      args: [appDir],
      env: {
        ...process.env,
        GOOSE_EXTERNAL_BACKEND: '',
        GOOSE_PATH_ROOT: profile,
        GOOSE_LOCALE: 'en',
      },
    });
    const peer = createAcpPeer();
    await electron.context().routeWebSocket(/\/acp(?:\?|$)/, peer.connect);
    await electron.evaluate(({ app }, url) => app.emit('start-recipe-test', url), startupLink);
    return { electron, peer, close };
  } catch (error) {
    await close();
    throw error;
  }
}

test('opens a parameterized link in the focused chat while its existing run continues', async ({}, testInfo) => {
  const { electron, peer, close } = await launchDesktop(testInfo);
  try {
    const first = await electron.firstWindow();
    await expect(first.getByTestId('chat-input')).toBeVisible();
    const cwd = testInfo.outputPath('focused-workdir');
    await mkdir(cwd);
    const nextWindow = electron.waitForEvent('window');
    await first.evaluate((dir) => window.electron.createChatWindow({ dir }), cwd);
    const second = await nextWindow;
    const input = second.getByTestId('chat-input');
    await input.fill('Keep this run going');
    await input.press('Enter');
    await expect
      .poll(() => peer.prompts)
      .toContainEqual({
        sessionId: 'original',
        text: 'Keep this run going',
      });

    const selectedDir = testInfo.outputPath('selected-workdir');
    await mkdir(selectedDir);
    await second.evaluate((dir) => window.electron.addRecentDir(dir), selectedDir);
    await second.getByRole('button', { name: 'focused-workdir', exact: true }).click();
    await second.getByRole('menuitem', { name: /selected-workdir/ }).click();
    await expect(second.getByRole('button', { name: 'selected-workdir', exact: true })).toBeVisible();

    await electron.evaluate(({ app, BrowserWindow }) => {
      const target = BrowserWindow.getAllWindows().find((win) =>
        win.webContents.getURL().includes('resumeSessionId=original')
      )!;
      target.focus();
      const link = 'goose://recipe?config=warm&topic=prefilled&scheduledJob=daily-recipe';
      if (process.platform === 'darwin') app.emit('open-url', {}, link);
      else app.emit('second-instance', {}, [link]);
    });
    await second.getByRole('button', { name: 'Trust and Execute' }).click();
    await expect(second.getByRole('button', { name: 'Trust and Execute' })).toBeHidden();
    const topic = second.getByRole('textbox', { name: /topic/i });
    await expect(topic).toHaveValue('prefilled');
    await topic.fill('resolved topic');
    await second.getByRole('button', { name: 'Start Recipe' }).click();
    await expect(second.getByText('Recipe Parameters', { exact: true })).toBeHidden();
    await expect(second).toHaveURL(/#\/pair\?resumeSessionId=warm-session$/);
    await expect
      .poll(() => peer.prompts)
      .toContainEqual({
        sessionId: 'warm-session',
        text: 'Write about resolved topic.',
      });
    expect(peer.launches).toEqual([
      expect.objectContaining({
        cwd: selectedDir,
        _meta: expect.objectContaining({
          recipeDeeplink: 'warm',
          scheduledJobId: 'daily-recipe',
          recipeParameterScopeId: expect.any(String),
        }),
      }),
    ]);
    expect(electron.windows()).toHaveLength(2);
    expect(peer.cancelledRuns).toEqual([]);

    peer.finishOriginalRun();
    await second.evaluate(() => {
      location.hash = '/pair?resumeSessionId=original';
    });
    await expect(second.getByText('Original run finished')).toBeVisible();
  } finally {
    await close();
  }
});

test('preserves cold launch metadata and keeps later parameterless links empty', async ({}, testInfo) => {
  const { electron, peer, close } = await launchDesktop(
    testInfo,
    'goose://recipe?config=cold&topic=startup&scheduledJob=cold-job'
  );
  try {
    const page = await electron.firstWindow();
    await page.getByRole('button', { name: 'Trust and Execute' }).click();
    const topic = page.getByRole('textbox', { name: /topic/i });
    await expect(topic).toHaveValue('startup');
    await page.getByRole('button', { name: 'Start Recipe' }).click();
    await expect(page).toHaveURL(/#\/pair\?resumeSessionId=cold-session$/);
    expect(peer.launches[0]._meta?.scheduledJobId).toBe('cold-job');
    expect(electron.windows()).toHaveLength(1);

    await electron.evaluate(({ app, BrowserWindow }) => {
      const window = BrowserWindow.getFocusedWindow() ?? BrowserWindow.getAllWindows()[0];
      window.webContents.once('did-start-navigation', () => {
        for (const link of [
          'goose://recipe?config=queued&topic=queued-value',
          'goose://recipe?config=parameterless',
        ]) {
          if (process.platform === 'darwin') app.emit('open-url', {}, link);
          else app.emit('second-instance', {}, [link]);
        }
      });
      window.reload();
    });
    await expect.poll(() => peer.launches.length).toBe(3);
    await expect(topic).toHaveValue('queued-value');
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.getByRole('button', { name: 'Start New Chat (No Recipe)' }).click();
    await expect(topic).toHaveValue('');
    await topic.fill('another topic');
    await page.getByRole('button', { name: 'Start Recipe' }).click();
    await expect(page.getByText('Recipe Parameters', { exact: true })).toBeHidden();
    await expect(page).toHaveURL(/#\/pair\?resumeSessionId=parameterless-session$/);
    expect(peer.prompts.map((prompt) => prompt.sessionId)).not.toContain('queued-session');
    expect(peer.decodedLinks).toEqual(['cold', 'queued', 'parameterless']);
    const parameterless = peer.launches.find(
      (launch) => launch._meta?.recipeDeeplink === 'parameterless'
    );
    expect(parameterless?._meta?.recipeParameterScopeId).toBeUndefined();
  } finally {
    await close();
  }
});
