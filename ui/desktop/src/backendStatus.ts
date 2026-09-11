import {
  acpHttpUrlFromHttpBase,
  normalizeAcpHttpBaseUrl,
  statusHttpUrlFromHttpBase,
} from './acp/url';
import {
  RedirectError,
  resolveRedirects,
  type HopRequest,
  type HopResponse,
  type RedirectPolicy,
} from './backendRedirects';

const RETRY_BUDGET_MS = 15000;
const RETRY_INTERVAL_MS = 250;
const PROBE_TIMEOUT_MS = 5000;

const FATAL_ERROR_PATTERN = /panicked at|RUST_BACKTRACE|fatal error/i;
const FATAL_NETWORK_PATTERN = /NAME_NOT_RESOLVED|CERT|SSL|CLIENT_AUTH|INVALID_URL|UNSAFE_PORT/;

export interface BackendCheckStep {
  name: string;
  ok: boolean;
  detail: string;
}

export interface BackendCheckResult {
  ok: boolean;
  steps: BackendCheckStep[];
  failure: string | null;
  resolvedAcpUrl: string | null;
}

export interface BackendCheckParams {
  baseUrl: string;
  serverSecret: string;
  request: HopRequest;
  pinnedHostname?: string | null;
  errorLog?: string[];
}

interface Probe {
  ok: boolean;
  detail: string;
  retryable: boolean;
  resolvedUrl?: string;
}

const delay = (timeoutMs: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, timeoutMs));

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const proxyNote = (response: HopResponse): string => {
  const doormanError = response.headers.get('x-sq-cf-doorman-error');
  return doormanError && doormanError !== 'none'
    ? ` A proxy in front of the backend reported "${doormanError}".`
    : '';
};

// Redirects are resolved anonymously; only the resolved URL is re-requested
// with the secret, so no intermediate origin ever sees it.
const probe = async (
  request: HopRequest,
  url: string,
  policy: RedirectPolicy,
  credentials: Record<string, string> | null,
  expect: (response: HopResponse, resolvedUrl: string) => Probe
): Promise<Probe> => {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), PROBE_TIMEOUT_MS);
  try {
    const { url: resolvedUrl, response } = await resolveRedirects(request, url, policy, {
      signal: controller.signal,
    });
    const finalResponse = credentials
      ? await request(resolvedUrl, { headers: credentials, signal: controller.signal })
      : response;
    return expect(finalResponse, resolvedUrl);
  } catch (error) {
    const detail = errorText(error);
    return {
      ok: false,
      detail,
      retryable: !(error instanceof RedirectError) && !FATAL_NETWORK_PATTERN.test(detail),
    };
  } finally {
    clearTimeout(timeout);
  }
};

const probeStatus = (
  request: HopRequest,
  baseUrl: string,
  policy: RedirectPolicy
): Promise<Probe> =>
  probe(request, statusHttpUrlFromHttpBase(baseUrl), policy, null, (response, resolvedUrl) =>
    response.status >= 200 && response.status < 300
      ? {
          ok: true,
          detail: `GET /status returned ${response.status}.`,
          retryable: false,
          resolvedUrl,
        }
      : {
          ok: false,
          detail: `GET /status returned ${response.status} ${response.statusText}.${proxyNote(response)}`,
          retryable: response.status >= 500,
        }
  );

const probeAcp = (
  request: HopRequest,
  baseUrl: string,
  secret: string,
  policy: RedirectPolicy
): Promise<Probe> =>
  probe(
    request,
    acpHttpUrlFromHttpBase(baseUrl),
    policy,
    { 'X-Secret-Key': secret },
    (response, resolvedUrl) => {
      if (response.status === 406) {
        return {
          ok: true,
          detail: 'The backend accepted the secret key.',
          retryable: false,
          resolvedUrl,
        };
      }
      if (response.status === 401 || response.status === 403) {
        return {
          ok: false,
          detail: `The backend rejected the secret key (HTTP ${response.status}). It must match GOOSE_SERVER__SECRET_KEY on the backend.${proxyNote(response)}`,
          retryable: false,
        };
      }
      return {
        ok: false,
        detail: `GET /acp returned ${response.status} ${response.statusText}, expected 406.${proxyNote(response)}`,
        retryable: response.status >= 500,
      };
    }
  );

const baseUrlFromStatusUrl = (statusUrl: string): string | null => {
  const url = new URL(statusUrl);
  const pathname = url.pathname.replace(/\/+$/, '');
  return pathname.endsWith('/status')
    ? `${url.origin}${pathname.slice(0, -'/status'.length)}`
    : null;
};

// The resolved endpoint is kept whole, including any query parameters a proxy
// added, so the socket connects exactly where the probe succeeded.
const acpWebSocketUrl = (acpUrl: string, secret: string): string => {
  const url = new URL(acpUrl);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', secret);
  return url.toString();
};

export const isFatalError = (line: string): boolean => FATAL_ERROR_PATTERN.test(line);

export const checkBackendStatus = async ({
  baseUrl,
  serverSecret,
  request,
  pinnedHostname = null,
  errorLog = [],
}: BackendCheckParams): Promise<BackendCheckResult> => {
  const steps: BackendCheckStep[] = [];
  const policy: RedirectPolicy = { pinnedHostname: pinnedHostname?.toLowerCase() ?? null };

  const run = async (name: string, attempt: () => Promise<Probe>): Promise<Probe> => {
    const deadline = Date.now() + RETRY_BUDGET_MS;
    let result = await attempt();
    while (
      !result.ok &&
      result.retryable &&
      Date.now() < deadline &&
      !errorLog.some(isFatalError)
    ) {
      await delay(RETRY_INTERVAL_MS);
      result = await attempt();
    }
    steps.push({ name, ok: result.ok, detail: result.detail });
    return result;
  };

  let normalizedBaseUrl = '';
  try {
    normalizedBaseUrl = normalizeAcpHttpBaseUrl(baseUrl);
    steps.push({ name: 'URL', ok: true, detail: normalizedBaseUrl });
  } catch (error) {
    steps.push({ name: 'URL', ok: false, detail: errorText(error) });
  }

  const resolve = async (): Promise<string | null> => {
    if (!normalizedBaseUrl) {
      return null;
    }

    const reachable = await run('Reachable', () => probeStatus(request, normalizedBaseUrl, policy));
    if (!reachable.ok || !reachable.resolvedUrl) {
      return null;
    }

    const resolvedBaseUrl = baseUrlFromStatusUrl(reachable.resolvedUrl);
    if (!resolvedBaseUrl) {
      steps.push({
        name: 'Redirect',
        ok: false,
        detail: `/status resolved to ${reachable.resolvedUrl}, so the ACP path cannot be derived from it.`,
      });
      return null;
    }
    if (resolvedBaseUrl !== normalizedBaseUrl) {
      steps.push({ name: 'Redirect', ok: true, detail: `Followed to ${resolvedBaseUrl}.` });
    }

    const accepted = await run('Secret key', () =>
      probeAcp(request, resolvedBaseUrl, serverSecret, policy)
    );
    if (!accepted.ok || !accepted.resolvedUrl) {
      return null;
    }

    const statusOrigin = new URL(resolvedBaseUrl).origin;
    const acpOrigin = new URL(accepted.resolvedUrl).origin;
    if (statusOrigin !== acpOrigin) {
      steps.push({
        name: 'Redirect',
        ok: false,
        detail: `/status and /acp resolved to different origins (${statusOrigin} and ${acpOrigin}).`,
      });
      return null;
    }

    return acpWebSocketUrl(accepted.resolvedUrl, serverSecret);
  };

  const resolvedAcpUrl = await resolve();

  const failed = steps.find((step) => !step.ok);
  return {
    ok: !failed,
    steps,
    failure: failed ? `${failed.name}: ${failed.detail}`.trim() : null,
    resolvedAcpUrl: failed ? null : resolvedAcpUrl,
  };
};
