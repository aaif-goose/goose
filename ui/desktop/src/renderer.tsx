import React, { Suspense, lazy } from 'react';
import ReactDOM from 'react-dom/client';
import { IntlProvider } from 'react-intl';
import { ConfigProvider } from './components/ConfigContext';
import { ErrorBoundary } from './components/ErrorBoundary';
import SuspenseLoader from './suspense-loader';
import { applyThemeTokens } from './theme/theme-tokens';
import { currentLocale, currentMessageLocale, loadMessages } from './i18n';

// Apply theme tokens to :root before first paint.
applyThemeTokens();

const App = lazy(() => import('./App'));

let warnedFallbackLocale = false;
function handleIntlError(err: { code: string; message?: string }) {
  if (err.code === 'MISSING_TRANSLATION' && currentLocale !== currentMessageLocale) {
    if (!warnedFallbackLocale) {
      warnedFallbackLocale = true;
      console.warn(
        `[i18n] Locale "${currentLocale}" has no translations; falling back to "${currentMessageLocale}".`
      );
    }
    return;
  }
  console.error(err);
}

(async () => {
  const messages = await loadMessages(currentMessageLocale);

  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <IntlProvider
        locale={currentLocale}
        defaultLocale="en"
        messages={messages}
        onError={handleIntlError}
      >
        <Suspense fallback={SuspenseLoader()}>
          <ConfigProvider>
            <ErrorBoundary>
              <App />
            </ErrorBoundary>
          </ConfigProvider>
        </Suspense>
      </IntlProvider>
    </React.StrictMode>
  );
})();
