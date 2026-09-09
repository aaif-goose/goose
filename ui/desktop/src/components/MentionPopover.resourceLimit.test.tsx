import { act, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import MentionPopover from './MentionPopover';

vi.mock('../acp/autocomplete', () => ({
  listAgentMentionItems: vi.fn().mockResolvedValue([]),
  listSlashCommandItems: vi.fn().mockResolvedValue([]),
}));

const props = {
  onClose: vi.fn(),
  onSelect: vi.fn(),
  position: { x: 0, y: 0 },
  query: '',
  isSlashCommand: false,
  selectedIndex: -1,
  onSelectedIndexChange: vi.fn(),
  workingDir: '/workspace',
};

describe('MentionPopover scan resource limits', () => {
  it('caps filesystem operations across a high-fanout tree', async () => {
    const listFiles = vi.fn(async (directory: string) => {
      const depth = directory.slice(props.workingDir.length).split('/').filter(Boolean).length;
      return depth < 4 ? ['dir-a', 'dir-b', 'dir-c', 'dir-d'] : [];
    });
    window.electron.listFiles = listFiles;

    render(<MentionPopover {...props} isOpen />, { wrapper: IntlTestWrapper });

    await waitFor(() => expect(screen.queryByText('Scanning files...')).not.toBeInTheDocument());
    expect(listFiles.mock.calls.length).toBeLessThanOrEqual(100);
  });

  it('stops scheduling filesystem operations after the popover closes', async () => {
    let resolveRoot!: (entries: string[]) => void;
    const rootEntries = new Promise<string[]>((resolve) => {
      resolveRoot = resolve;
    });
    const listFiles = vi.fn((directory: string) =>
      directory === props.workingDir ? rootEntries : Promise.resolve([])
    );
    window.electron.listFiles = listFiles;

    const { rerender } = render(<MentionPopover {...props} isOpen />, {
      wrapper: IntlTestWrapper,
    });
    await waitFor(() => expect(listFiles).toHaveBeenCalledTimes(1));

    rerender(<MentionPopover {...props} isOpen={false} />);
    await act(async () => {
      resolveRoot(['child']);
      await rootEntries;
    });

    expect(listFiles).toHaveBeenCalledTimes(1);
  });

  it('keeps returning files and directories in ordinary trees', async () => {
    const listFiles = vi.fn(async (directory: string) => {
      if (directory === props.workingDir) return ['src', 'README.md'];
      if (directory === `${props.workingDir}/src`) return ['index.ts'];
      return [];
    });
    window.electron.listFiles = listFiles;

    render(<MentionPopover {...props} isOpen />, { wrapper: IntlTestWrapper });

    expect(await screen.findByText('README.md')).toBeInTheDocument();
    expect(await screen.findByText('index.ts')).toBeInTheDocument();
    expect(listFiles).toHaveBeenCalledTimes(3);
  });
});
