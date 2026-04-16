import { describe, it, expect, vi, beforeEach } from 'vitest';
import { addExtensionFromDeepLink } from './deeplink';

vi.mock('../../../toasts', () => ({
  toastService: {
    handleError: vi.fn(),
    success: vi.fn(),
  },
}));

describe('addExtensionFromDeepLink', () => {
  const mockAddExtension = vi.fn().mockResolvedValue(undefined);
  const mockSetView = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('header parsing', () => {
    it('should preserve = characters in header values', async () => {
      const url =
        'goose://extension?name=Remote&url=https%3A%2F%2Fexample.com%2Fmcp&header=Authorization%3DBasic%20abc%3D%3D';

      await addExtensionFromDeepLink(url, mockAddExtension, mockSetView);

      expect(mockSetView).toHaveBeenCalledWith(
        'extensions',
        expect.objectContaining({
          showEnvVars: true,
          deepLinkConfig: expect.objectContaining({
            headers: { Authorization: 'Basic abc==' },
          }),
        })
      );
    });

    it('should handle header values without = characters', async () => {
      const url =
        'goose://extension?name=Remote&url=https%3A%2F%2Fexample.com%2Fmcp&header=X-Token%3Dabc123';

      await addExtensionFromDeepLink(url, mockAddExtension, mockSetView);

      expect(mockSetView).toHaveBeenCalledWith(
        'extensions',
        expect.objectContaining({
          deepLinkConfig: expect.objectContaining({
            headers: { 'X-Token': 'abc123' },
          }),
        })
      );
    });

    it('should handle multiple headers', async () => {
      const url =
        'goose://extension?name=Remote&url=https%3A%2F%2Fexample.com%2Fmcp&header=Authorization%3DBearer%20tok%3D%3D&header=X-Key%3Dval';

      await addExtensionFromDeepLink(url, mockAddExtension, mockSetView);

      expect(mockSetView).toHaveBeenCalledWith(
        'extensions',
        expect.objectContaining({
          deepLinkConfig: expect.objectContaining({
            headers: {
              Authorization: 'Bearer tok==',
              'X-Key': 'val',
            },
          }),
        })
      );
    });

    it('should handle header with empty value', async () => {
      const url =
        'goose://extension?name=Remote&url=https%3A%2F%2Fexample.com%2Fmcp&header=X-Empty%3D';

      await addExtensionFromDeepLink(url, mockAddExtension, mockSetView);

      expect(mockSetView).toHaveBeenCalledWith(
        'extensions',
        expect.objectContaining({
          deepLinkConfig: expect.objectContaining({
            headers: { 'X-Empty': '' },
          }),
        })
      );
    });
  });
});
