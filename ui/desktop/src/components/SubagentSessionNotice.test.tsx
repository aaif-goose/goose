import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import { SubagentSessionNotice } from './SubagentSessionNotice';

describe('SubagentSessionNotice', () => {
  it('explains the view is not live and loads the latest progress on click', () => {
    const onRefresh = vi.fn();
    render(<SubagentSessionNotice refreshing={false} onRefresh={onRefresh} />, {
      wrapper: IntlTestWrapper,
    });

    expect(screen.getByRole('status')).toHaveTextContent(
      "Subagent progress isn't shown live here."
    );
    fireEvent.click(screen.getByRole('button', { name: 'Load latest progress' }));
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it('disables loading while a load is in progress', () => {
    render(<SubagentSessionNotice refreshing onRefresh={vi.fn()} />, {
      wrapper: IntlTestWrapper,
    });

    expect(screen.getByRole('button', { name: 'Load latest progress' })).toBeDisabled();
  });
});
