import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { IntlTestWrapper } from '../i18n/test-utils';
import InferenceSecurityBadge from './InferenceSecurityBadge';

describe('InferenceSecurityBadge', () => {
  it('only displays a badge for a verified attested response', () => {
    const { rerender } = render(<InferenceSecurityBadge security={undefined} />, {
      wrapper: IntlTestWrapper,
    });
    expect(screen.queryByText('Verified TEE')).not.toBeInTheDocument();
    rerender(<InferenceSecurityBadge security="attested_tee" />);
    expect(screen.getByText('Verified TEE')).toBeInTheDocument();
    rerender(<InferenceSecurityBadge security={null} />);
    expect(screen.queryByText('Verified TEE')).not.toBeInTheDocument();
  });

  it('renders a plain label without a separate tooltip', async () => {
    render(<InferenceSecurityBadge security="attested_tee" />, { wrapper: IntlTestWrapper });
    await userEvent.hover(screen.getByText('Verified TEE'));
    expect(screen.queryByRole('tooltip')).not.toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});
