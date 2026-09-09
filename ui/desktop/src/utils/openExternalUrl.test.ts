import { dialog, shell } from 'electron';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { openExternalUrl } from './openExternalUrl';

vi.mock('electron', () => ({
  dialog: { showMessageBox: vi.fn() },
  shell: { openExternal: vi.fn() },
}));

describe('openExternalUrl', () => {
  beforeEach(() => {
    vi.mocked(dialog.showMessageBox).mockReset();
    vi.mocked(shell.openExternal).mockReset();
    vi.mocked(shell.openExternal).mockResolvedValue(undefined);
  });

  it('opens allowlisted protocols without confirmation', async () => {
    await expect(openExternalUrl('https://example.com/docs')).resolves.toBe('opened');

    expect(dialog.showMessageBox).not.toHaveBeenCalled();
    expect(shell.openExternal).toHaveBeenCalledWith('https://example.com/docs');
  });

  it.each(['file:///tmp/secret', 'javascript:alert(1)', 'not a URL'])(
    'blocks dangerous or malformed URL %s',
    async (url) => {
      await expect(openExternalUrl(url)).resolves.toBe('blocked');

      expect(dialog.showMessageBox).not.toHaveBeenCalled();
      expect(shell.openExternal).not.toHaveBeenCalled();
    }
  );

  it('does not open an unknown protocol when confirmation is cancelled', async () => {
    vi.mocked(dialog.showMessageBox).mockResolvedValue({ response: 0, checkboxChecked: false });

    await expect(openExternalUrl('ms-msdt:exploit')).resolves.toBe('cancelled');

    expect(dialog.showMessageBox).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultId: 0,
        cancelId: 0,
        title: 'Open External Link',
        message: 'Open ms-msdt: link?',
        detail: 'This will open: ms-msdt:exploit',
      })
    );
    expect(shell.openExternal).not.toHaveBeenCalled();
  });

  it('opens an unknown protocol only after explicit confirmation', async () => {
    vi.mocked(dialog.showMessageBox).mockResolvedValue({ response: 1, checkboxChecked: false });

    await expect(openExternalUrl('custom-handler:resource')).resolves.toBe('opened');

    expect(shell.openExternal).toHaveBeenCalledWith('custom-handler:resource');
  });

  it('uses the configured locale for an unknown-protocol confirmation', async () => {
    vi.mocked(dialog.showMessageBox).mockResolvedValue({ response: 0, checkboxChecked: false });

    await openExternalUrl('custom-handler:resource', undefined, 'es-ES');

    expect(dialog.showMessageBox).toHaveBeenCalledWith(
      expect.objectContaining({
        buttons: ['Cancelar', 'Abrir'],
        title: 'Abrir enlace externo',
        message: '¿Abrir enlace custom-handler:?',
        detail: 'Esto abrirá: custom-handler:resource',
      })
    );
  });
});
