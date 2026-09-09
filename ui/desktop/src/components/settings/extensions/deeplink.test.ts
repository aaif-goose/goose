import { describe, it, expect, vi, beforeEach } from 'vitest';
import { addExtensionFromDeepLink } from './deeplink';
import { toastService } from '../../../toasts';

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

  describe('stdio command validation', () => {
    it('should allow goose for bundled MCP deeplinks', async () => {
      const url =
        'goose://extension?cmd=goose&arg=mcp&arg=memory&name=Memory&description=Memory';

      await addExtensionFromDeepLink(url, mockAddExtension, mockSetView);

      expect(mockAddExtension).toHaveBeenCalledWith(
        'Memory',
        expect.objectContaining({
          type: 'stdio',
          cmd: 'goose',
          args: ['mcp', 'memory'],
        }),
        true
      );
    });

    it('should reject legacy goosed deeplinks', async () => {
      vi.mocked(toastService.handleError).mockImplementationOnce(() => {
        throw new Error('Invalid command');
      });

      const url =
        'goose://extension?cmd=goosed&arg=mcp&arg=memory&name=Memory&description=Memory';

      await expect(addExtensionFromDeepLink(url, mockAddExtension, mockSetView)).rejects.toThrow(
        'Invalid command'
      );

      expect(toastService.handleError).toHaveBeenCalledWith(
        'Invalid Command',
        expect.stringContaining('Invalid command: goosed'),
        { shouldThrow: true }
      );
      expect(mockAddExtension).not.toHaveBeenCalled();
    });
  });
});
