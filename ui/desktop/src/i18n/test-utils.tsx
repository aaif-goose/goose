import React from 'react';
import { IntlProvider } from 'react-intl';

/**
 * Wraps a component tree with IntlProvider for tests.
 * Uses English locale with no messages (defaultMessage values are used).
 */
export function IntlTestWrapper({ children }: { children: React.ReactNode }) {
  return (
    <IntlProvider locale="en" defaultLocale="en" messages={{}}>
      {children}
    </IntlProvider>
  );
}
