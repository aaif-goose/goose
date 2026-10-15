import https from 'node:https';

export function fetchAllowlistContent(url: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const follow = (currentUrl: string, depth: number) => {
      if (depth > 10) {
        reject(new Error('Too many redirects fetching allowlist'));
        return;
      }
      let parsed: URL;
      try {
        parsed = new URL(currentUrl);
      } catch {
        reject(new Error(`Invalid allowlist URL: ${currentUrl}`));
        return;
      }
      if (parsed.protocol !== 'https:') {
        reject(new Error(`GOOSE_ALLOWLIST must use HTTPS: ${currentUrl}`));
        return;
      }
      https
        .get(currentUrl, (res) => {
          const { statusCode } = res;
          if (statusCode && statusCode >= 300 && statusCode < 400) {
            const location = res.headers.location;
            if (!location) {
              reject(new Error('Allowlist redirect missing Location header'));
              return;
            }
            res.resume();
            follow(new URL(location, currentUrl).href, depth + 1);
          } else if (statusCode && statusCode >= 200 && statusCode < 300) {
            let data = '';
            res.setEncoding('utf8');
            res.on('data', (chunk: string) => {
              data += chunk;
            });
            res.on('end', () => resolve(data));
            res.on('error', reject);
          } else {
            res.resume();
            reject(new Error(`Allowlist fetch failed: HTTP ${statusCode}`));
          }
        })
        .on('error', reject);
    };
    follow(url, 0);
  });
}
