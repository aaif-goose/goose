export function clientExtensionViewPath(extensionId: string, viewId: string): string {
  return `/ext/${encodeURIComponent(extensionId)}/${encodeURIComponent(viewId)}`;
}

export function parseClientExtensionViewPath(pathname: string): {
  extensionId: string;
  viewId: string;
} | null {
  const match = pathname.match(/^\/ext\/([^/]+)\/([^/]+)$/);
  if (!match) {
    return null;
  }
  return {
    extensionId: decodeURIComponent(match[1]),
    viewId: decodeURIComponent(match[2]),
  };
}
