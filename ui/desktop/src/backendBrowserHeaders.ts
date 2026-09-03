import type { Session } from 'electron';

type HeaderRewritingSession = Pick<Session, 'webRequest'>;

const CHROMIUM_BRAND = /"Chromium";v="(\d+)"/;

// Some corporate proxies (Cloudflare Doorman) only admit requests they can
// attribute to a supported browser, and reject Electron on two signals: the
// Electron/<version> token in the User-Agent, and a sec-ch-ua brand list that
// advertises Chromium without Google Chrome. Either one is answered with
// "unsupported_browser" before the client certificate is ever evaluated, so
// both are normalised for backend requests.
export const applyBrowserRequestHeaders = (targetSession: HeaderRewritingSession): void => {
  targetSession.webRequest.onBeforeSendHeaders((details, callback) => {
    const requestHeaders = { ...details.requestHeaders };

    for (const name of Object.keys(requestHeaders)) {
      if (name.toLowerCase() === 'user-agent') {
        requestHeaders[name] = requestHeaders[name].replace(/ Electron\/\S+/, '');
        continue;
      }

      if (name.toLowerCase() !== 'sec-ch-ua') {
        continue;
      }

      const version = CHROMIUM_BRAND.exec(requestHeaders[name])?.[1];
      if (version) {
        requestHeaders[name] =
          `"Not;A=Brand";v="8", "Chromium";v="${version}", "Google Chrome";v="${version}"`;
      }
    }

    callback({ requestHeaders });
  });
};
