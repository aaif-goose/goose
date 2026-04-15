/**
 * Locale detection and message loading for the i18n system.
 *
 * Locale resolution order:
 *   1. GOOSE_LOCALE config value (set via environment variable, passed through appConfig)
 *   2. navigator.language (browser/OS locale)
 *   3. "en" (fallback)
 */

// Re-export react-intl utilities that components use directly
export { defineMessages, useIntl } from 'react-intl';

/** The set of locales that have translation catalogs. */
const SUPPORTED_LOCALES = new Set(['en']);

/**
 * Detect the user's preferred locale.
 *
 * Returns two values:
 * - `locale`: the full BCP 47 tag (e.g. "en-GB") for formatting (dates, numbers).
 * - `messageLocale`: the base language that has a translation catalog (e.g. "en").
 */
export function getLocale(): { locale: string; messageLocale: string } {
  const explicit =
    typeof window !== 'undefined' && window.appConfig
      ? window.appConfig.get('GOOSE_LOCALE')
      : undefined;

  const candidates: string[] = [];

  if (typeof explicit === 'string' && explicit) {
    candidates.push(explicit);
  }

  if (typeof navigator !== 'undefined' && navigator.language) {
    candidates.push(navigator.language);
  }

  for (const tag of candidates) {
    // Exact match first
    if (SUPPORTED_LOCALES.has(tag)) return { locale: tag, messageLocale: tag };
    // Try base language (e.g. "pt-BR" → "pt") for the catalog, but keep the
    // full regional tag for formatting so date/number output respects the region.
    const base = tag.split('-')[0];
    if (SUPPORTED_LOCALES.has(base)) {
      // Validate the full tag is a well-formed BCP 47 locale before using it
      // for formatting. Invalid tags (e.g. "en-") would cause RangeError in
      // Intl APIs, so fall back to the base language in that case.
      let locale = base;
      try {
        [locale] = Intl.getCanonicalLocales(tag);
      } catch {
        // tag is not valid BCP 47 — use the base language instead
      }
      return { locale, messageLocale: base };
    }
  }

  return { locale: 'en', messageLocale: 'en' };
}

/** Resolved locales — computed once at module load. */
const resolvedLocale = getLocale();
/** Full BCP 47 tag for date/number formatting (e.g. "en-GB"). */
export const currentLocale = resolvedLocale.locale;
/** Base language for loading message catalogs (e.g. "en"). */
export const currentMessageLocale = resolvedLocale.messageLocale;

/**
 * Load compiled messages for a given locale.
 * Returns an empty object for English (react-intl uses defaultMessage as fallback).
 */
export async function loadMessages(
  locale: string
): Promise<Record<string, string>> {
  if (locale === 'en') {
    // English strings live in source code as defaultMessage — no catalog needed.
    return {};
  }

  try {
    // Dynamic import so compiled translation bundles are code-split.
    const mod = await import(`./compiled/${locale}.json`);
    return mod.default ?? mod;
  } catch {
    console.warn(`[i18n] No message catalog found for locale "${locale}", falling back to English.`);
    return {};
  }
}
