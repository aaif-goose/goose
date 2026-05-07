import { render, screen, type RenderOptions, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../../../i18n/test-utils';
import { CronPicker } from '../CronPicker';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const getLastCron = (onChange: ReturnType<typeof vi.fn>) => {
  const calls = onChange.mock.calls;
  return calls[calls.length - 1]?.[0];
};

describe('CronPicker', () => {
  it('generates quarterly cron expressions from the quarter preset', async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();

    renderWithIntl(<CronPicker schedule={null} onChange={onChange} isValid={vi.fn()} />);

    await user.selectOptions(screen.getAllByRole('combobox')[0], 'quarter');

    await waitFor(() => {
      expect(getLastCron(onChange)).toBe('0 0 14 1 1,4,7,10 *');
    });

    const dayInput = screen.getAllByRole('spinbutton')[0];
    await user.clear(dayInput);
    await user.type(dayInput, '31');
    await user.selectOptions(screen.getAllByRole('combobox')[1], '2');

    await waitFor(() => {
      expect(dayInput).toHaveValue(28);
      expect(getLastCron(onChange)).toBe('0 0 14 28 2,5,8,11 *');
    });
  });
});
