const MAX_REDIRECT_HOPS = 20;

export interface HopRequestInit {
  headers?: Record<string, string>;
  signal?: AbortSignal;
}

export interface HopResponse {
  status: number;
  statusText: string;
  headers: { get(name: string): string | null };
  location: string | null;
}

export type HopRequest = (url: string, init: HopRequestInit) => Promise<HopResponse>;

export interface RedirectPolicy {
  pinnedHostname: string | null;
}

export interface ResolvedRequest {
  url: string;
  response: HopResponse;
}

export class RedirectError extends Error {}

const isRedirect = (status: number): boolean =>
  status === 301 || status === 302 || status === 303 || status === 307 || status === 308;

const checkHop = (from: URL, to: URL, policy: RedirectPolicy): void => {
  if (to.protocol !== 'http:' && to.protocol !== 'https:') {
    throw new RedirectError(`Redirect to ${to.protocol} is not allowed, only http: and https:.`);
  }
  if (from.protocol === 'https:' && to.protocol === 'http:') {
    throw new RedirectError(
      `Redirect from HTTPS to HTTP is not allowed (${from.origin} to ${to.origin}).`
    );
  }
  if (policy.pinnedHostname && to.hostname.toLowerCase() !== policy.pinnedHostname) {
    throw new RedirectError(
      `Redirect to ${to.hostname} is not allowed because a certificate fingerprint is configured for ${policy.pinnedHostname}.`
    );
  }
};

// Hops carry no credentials; the caller re-requests the resolved URL with the
// secret once every hop has been validated.
export const resolveRedirects = async (
  request: HopRequest,
  startUrl: string,
  policy: RedirectPolicy,
  init: HopRequestInit = {}
): Promise<ResolvedRequest> => {
  let url = new URL(startUrl);

  for (let hop = 0; ; hop += 1) {
    const response = await request(url.toString(), init);
    if (!isRedirect(response.status)) {
      return { url: url.toString(), response };
    }

    if (hop >= MAX_REDIRECT_HOPS) {
      throw new RedirectError(`Exceeded ${MAX_REDIRECT_HOPS} redirects starting at ${startUrl}.`);
    }
    if (!response.location) {
      throw new RedirectError(`Redirect from ${url.toString()} has no Location header.`);
    }

    const next = new URL(response.location, url);
    checkHop(url, next, policy);
    url = next;
  }
};
