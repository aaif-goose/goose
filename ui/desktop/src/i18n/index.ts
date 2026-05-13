/**
 * Locale detection and message loading for the i18n system.
 *
 * Locale resolution order:
 *   1. GOOSE_LOCALE config value (set via environment variable, passed through appConfig)
 *   2. navigator.languages (full accept-language list from OS/browser)
 *   3. "en" (fallback)
 *
 * For Chinese: any Simplified Chinese tag (zh, zh-CN, zh-Hans, zh-Hans-CN, zh-SG, zh-MY)
 * maps to the "zh-CN" catalog. Traditional variants (zh-TW, zh-HK, zh-Hant) are not yet
 * translated and fall through to English.
 */

// Re-export react-intl utilities that components use directly
export { defineMessages, useIntl } from 'react-intl';

/** The set of locales that have translation catalogs. */
const SUPPORTED_LOCALES = new Set(['en', 'zh-CN']);

/**
 * Map Simplified Chinese aliases (zh, zh-Hans*, zh-SG, zh-MY) to "zh-CN".
 * Traditional variants (zh-Hant*, zh-TW, zh-HK, zh-MO) and non-Chinese tags pass through unchanged.
 */
function resolveChineseAlias(tag: string): string {
  const lower = tag.toLowerCase();
  if (/^zh-(hant|tw|hk|mo)(-|$)/.test(lower)) return tag;
  if (lower === 'zh' || lower.startsWith('zh-')) return 'zh-CN';
  return tag;
}

/**
 * Detect the user's preferred locale.
 *
 * Returns two values:
 * - `locale`: the full BCP 47 tag (e.g. "en-GB") for formatting (dates, numbers).
 * - `messageLocale`: the locale key that has a translation catalog (e.g. "en", "zh-CN").
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

  // Walk navigator.languages (full preference list) so a user whose primary UI
  // language isn't supported still gets a supported language from later in their list.
  if (typeof navigator !== 'undefined' && Array.isArray(navigator.languages)) {
    for (const tag of navigator.languages) {
      if (tag) candidates.push(tag);
    }
  }

  for (const rawTag of candidates) {
    // Normalize underscores to hyphens so POSIX-style tags like "zh_CN" work.
    const normalized = rawTag.replace(/_/g, '-');
    const tag = resolveChineseAlias(normalized);

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
        [locale] = Intl.getCanonicalLocales(normalized);
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
export async function loadMessages(locale: string): Promise<Record<string, string>> {
  if (locale === 'en') {
    // English strings live in source code as defaultMessage — no catalog needed.
    return {};
  }

  try {
    // Dynamic import so compiled translation bundles are code-split.
    const mod = await import(`./compiled/${locale}.json`);
    return mod.default ?? mod;
  } catch {
    console.warn(
      `[i18n] No message catalog found for locale "${locale}", falling back to English.`
    );
    return {};
  }
}
