import { WebContentsView } from 'electron';

// Chromium only runs client certificate selection for WebContents-originated
// requests, so mTLS backends are unreachable from main-process net.fetch. Probes
// go through an offscreen view on the renderer partition to share the network
// path the ACP WebSocket uses. A WebContentsView is not a window, so it does not
// affect the window-all-closed lifecycle.
const PROBE_PARTITION = 'persist:goose';

type ProbeResult =
  | { ok: true; status: number; statusText: string; url: string; headers: [string, string][] }
  | { ok: false; message: string };

let probeView: WebContentsView | null = null;

const getProbeView = async (): Promise<WebContentsView> => {
  if (probeView && !probeView.webContents.isDestroyed()) {
    return probeView;
  }

  probeView = new WebContentsView({
    webPreferences: { partition: PROBE_PARTITION, nodeIntegration: false, contextIsolation: true },
  });
  await probeView.webContents.loadURL('data:text/html,<title>goose backend probe</title>');
  return probeView;
};

type FetchInit = NonNullable<Parameters<typeof globalThis.fetch>[1]>;

const rejectOnAbort = (signal: NonNullable<FetchInit['signal']>): Promise<never> =>
  new Promise((_resolve, reject) => {
    signal.addEventListener('abort', () => reject(new Error('Probe request timed out.')), {
      once: true,
    });
  });

export const probeFetch: typeof globalThis.fetch = async (input, init) => {
  const view = await getProbeView();
  const request = JSON.stringify({
    url: String(input),
    headers: (init?.headers as Record<string, string> | undefined) ?? {},
  });

  const probe = view.webContents.executeJavaScript(`
    (async () => {
      const request = ${request};
      try {
        const response = await fetch(request.url, { headers: request.headers });
        return {
          ok: true,
          status: response.status,
          statusText: response.statusText,
          url: response.url,
          headers: [...response.headers.entries()],
        };
      } catch (error) {
        return { ok: false, message: String((error && error.message) || error) };
      }
    })()
  `) as Promise<ProbeResult>;

  const result: ProbeResult = await (init?.signal
    ? Promise.race([probe, rejectOnAbort(init.signal)])
    : probe);
  if (result.ok !== true) {
    throw new Error(result.message);
  }

  return {
    ok: result.status >= 200 && result.status < 300,
    status: result.status,
    statusText: result.statusText,
    url: result.url,
    headers: new globalThis.Headers(result.headers),
  } as Response;
};

export const closeBackendProbe = (): void => {
  probeView?.webContents.close();
  probeView = null;
};
