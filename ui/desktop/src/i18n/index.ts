/**
 * Locale detection and message loading for the i18n system.
 *
 * Locale resolution order:
 *   1. GOOSE_LOCALE config value (set via environment variable, passed through appConfig)
 *   2. navigator.languages (full accept-language list from OS/browser)
 *   3. navigator.language (primary locale)
 *   4. "en" (fallback)
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
 * Normalize a BCP 47 tag to one of the SUPPORTED_LOCALES, or null if no match.
 *
 * Handles Simplified Chinese aliases: "zh", "zh-Hans", "zh-Hans-CN", "zh-SG", "zh-MY"
 * all map to "zh-CN". "zh-TW" / "zh-HK" / "zh-Hant*" are intentionally NOT matched —
 * they are Traditional Chinese and distinct from Simplified.
 */
function matchSupported(tag: string): string | null {
  if (!tag) return null;

  // Normalize underscores to hyphens so POSIX-style tags like "zh_CN" work.
  tag = tag.replace(/_/g, '-');

  // Exact match first
  if (SUPPORTED_LOCALES.has(tag)) return tag;

  const lower = tag.toLowerCase();

  // Simplified Chinese variants
  const isTraditional = /^zh-(hant|tw|hk|mo)\b/.test(lower);
  if (!isTraditional && (lower === 'zh' || lower.startsWith('zh-'))) {
    // Any zh-* that is NOT Traditional counts as Simplified.
    // Covers zh, zh-CN, zh-Hans, zh-Hans-CN, zh-SG, zh-MY.
    if (SUPPORTED_LOCALES.has('zh-CN')) return 'zh-CN';
  }

  // Base language fallback (e.g. "pt-BR" → "pt")
  const base = tag.split('-')[0];
  if (SUPPORTED_LOCALES.has(base)) return base;

  return null;
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

  if (typeof navigator !== 'undefined') {
    // Check navigator.languages (full preference list) first, then primary language.
    // This lets a Chinese user on an en-US Windows UI still get Chinese if zh is in their list.
    if (Array.isArray(navigator.languages)) {
      for (const tag of navigator.languages) {
        if (tag) candidates.push(tag);
      }
    }
    if (navigator.language) candidates.push(navigator.language);
  }

  for (const tag of candidates) {
    const matched = matchSupported(tag);
    if (!matched) continue;

    // Validate the full tag is a well-formed BCP 47 locale before using it
    // for formatting. Invalid tags (e.g. "en-") would cause RangeError in
    // Intl APIs, so fall back to the matched key in that case.
    let locale = matched;
    try {
      [locale] = Intl.getCanonicalLocales(tag);
    } catch {
      // tag is not valid BCP 47 — use the matched key instead
    }
    return { locale, messageLocale: matched };
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
