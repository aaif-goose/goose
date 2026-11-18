import https from 'node:https';
import * as yaml from 'yaml';

const ALLOWLIST_FETCH_TIMEOUT_MS = 30_000;

export function parseAllowlistCommands(content: string): string[] {
  const parsed: unknown = yaml.parse(content);

  if (
    !parsed ||
    typeof parsed !== 'object' ||
    !Array.isArray((parsed as { extensions?: unknown }).extensions)
  ) {
    throw new Error('Invalid GOOSE_ALLOWLIST policy: expected an extensions array');
  }

  return (parsed as { extensions: unknown[] }).extensions.map((extension, index) => {
    if (
      !extension ||
      typeof extension !== 'object' ||
      typeof (extension as { command?: unknown }).command !== 'string' ||
      (extension as { command: string }).command.trim() === ''
    ) {
      throw new Error(`Invalid GOOSE_ALLOWLIST policy: extension ${index} requires a command`);
    }

    return (extension as { command: string }).command;
  });
}

export function fetchAllowlistContent(url: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const controller = new AbortController();
    let settled = false;

    const finishResolve = (content: string) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      resolve(content);
      controller.abort();
    };

    const finishReject = (error: Error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      reject(error);
      controller.abort();
    };

    const timeout = setTimeout(() => {
      finishReject(new Error('Allowlist fetch timed out'));
    }, ALLOWLIST_FETCH_TIMEOUT_MS);

    const follow = (currentUrl: string, depth: number) => {
      if (settled) return;
      if (depth > 10) {
        finishReject(new Error('Too many redirects fetching allowlist'));
        return;
      }
      let parsed: URL;
      try {
        parsed = new URL(currentUrl);
      } catch {
        finishReject(new Error(`Invalid allowlist URL: ${currentUrl}`));
        return;
      }
      if (parsed.protocol !== 'https:') {
        finishReject(new Error(`GOOSE_ALLOWLIST must use HTTPS: ${currentUrl}`));
        return;
      }
      try {
        https
          .get(
            currentUrl,
            { signal: controller.signal, headers: { 'accept-encoding': 'identity' } },
            (res) => {
              if (settled) {
                res.resume();
                return;
              }
              const { statusCode } = res;
              if (statusCode && statusCode >= 300 && statusCode < 400) {
                const location = res.headers.location;
                if (!location) {
                  finishReject(new Error('Allowlist redirect missing Location header'));
                  return;
                }
                res.resume();
                let redirectUrl: string;
                try {
                  redirectUrl = new URL(location, currentUrl).href;
                } catch {
                  finishReject(new Error(`Invalid allowlist redirect URL: ${location}`));
                  return;
                }
                follow(redirectUrl, depth + 1);
              } else if (statusCode && statusCode >= 200 && statusCode < 300) {
                let data = '';
                res.setEncoding('utf8');
                res.on('data', (chunk: string) => {
                  if (!settled) data += chunk;
                });
                res.on('end', () => finishResolve(data));
                res.on('error', finishReject);
              } else {
                res.resume();
                finishReject(new Error(`Allowlist fetch failed: HTTP ${statusCode}`));
              }
            }
          )
          .on('error', finishReject);
      } catch (error) {
        finishReject(error instanceof Error ? error : new Error(String(error)));
      }
    };
    follow(url, 0);
  });
}
