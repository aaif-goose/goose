import { describe, it, expect, vi } from 'vitest';
import { render, type RenderOptions, screen, fireEvent, waitFor } from '@testing-library/react';
import ExtensionItem from './ExtensionItem';
import { IntlTestWrapper } from '../../../../i18n/test-utils';
import type { FixedExtensionEntry } from '../../../ConfigContext';

vi.mock('./ExtensionList', () => ({
  getSubtitle: () => ({ description: '', command: '' }),
  getFriendlyTitle: (ext: { name: string }) => ext.name,
}));

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const makeExtension = (enabled: boolean): FixedExtensionEntry =>
  ({ name: 'developer', type: 'builtin', enabled }) as unknown as FixedExtensionEntry;

describe('ExtensionItem', () => {
  it('reflects the toggle as OFF immediately when disabling, before the async toggle resolves', async () => {
    // onToggle stays pending so we observe the in-flight (optimistic) state
    const onToggle = vi.fn(() => new Promise<void>(() => {}));
    renderWithIntl(
      <ExtensionItem extension={makeExtension(true)} onToggle={onToggle} onConfigure={vi.fn()} />
    );

    const toggle = screen.getByRole('switch');
    expect(toggle).toHaveAttribute('aria-checked', 'true');

    fireEvent.click(toggle);

    await waitFor(() => {
      expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'false');
    });
  });

  it('reflects the toggle as ON immediately when enabling, before the async toggle resolves', async () => {
    const onToggle = vi.fn(() => new Promise<void>(() => {}));
    renderWithIntl(
      <ExtensionItem extension={makeExtension(false)} onToggle={onToggle} onConfigure={vi.fn()} />
    );

    const toggle = screen.getByRole('switch');
    expect(toggle).toHaveAttribute('aria-checked', 'false');

    fireEvent.click(toggle);

    await waitFor(() => {
      expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'true');
    });
  });
});
