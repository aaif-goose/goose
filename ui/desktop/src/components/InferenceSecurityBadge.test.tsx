import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import InferenceSecurityBadge from './InferenceSecurityBadge';

// Radix tooltip positioning needs ResizeObserver, which jsdom does not implement.
beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

afterAll(() => vi.unstubAllGlobals());

describe('InferenceSecurityBadge', () => {
  it('only displays a badge for a verified attested response', () => {
    const { rerender } = render(<InferenceSecurityBadge security={undefined} />, {
      wrapper: IntlTestWrapper,
    });
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    rerender(<InferenceSecurityBadge security="attested_tee" />);
    expect(screen.getByRole('button', { name: 'Attested TEE' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Attested TEE' })).toHaveTextContent(/^$/);
    rerender(<InferenceSecurityBadge security={null} />);
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('explains the scope of verification on keyboard focus', async () => {
    const user = userEvent.setup();
    render(<InferenceSecurityBadge security="attested_tee" />, { wrapper: IntlTestWrapper });
    await user.tab();
    expect(await screen.findByRole('tooltip')).toHaveTextContent(
      'The model connection for this response was bound to a verified trusted execution environment (TEE). Tool execution is outside this guarantee.'
    );
  });
});
