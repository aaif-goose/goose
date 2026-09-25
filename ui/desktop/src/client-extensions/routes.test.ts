import { describe, expect, it } from 'vitest';
import { clientExtensionViewPath, parseClientExtensionViewPath } from './routes';

describe('client extension routes', () => {
  it('builds encoded view paths', () => {
    expect(clientExtensionViewPath('hello-page', 'home')).toBe('/ext/hello-page/home');
  });

  it('parses view paths', () => {
    expect(parseClientExtensionViewPath('/ext/hello-page/home')).toEqual({
      extensionId: 'hello-page',
      viewId: 'home',
    });
  });

  it('returns null for non-extension paths', () => {
    expect(parseClientExtensionViewPath('/pair')).toBeNull();
  });
});
