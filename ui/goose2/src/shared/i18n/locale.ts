import {
  DEFAULT_LOCALE,
  LOCALE_STORAGE_KEY,
  SUPPORTED_LOCALES,
  SYSTEM_LOCALE,
  type AppLocale,
  type LocalePreference,
} from "./constants";

const SUPPORTED_LOCALE_SET = new Set<string>(SUPPORTED_LOCALES);

function safeCanonicalizeLocale(locale: string): string | null {
  try {
    return Intl.getCanonicalLocales(locale)[0] ?? null;
  } catch {
    return null;
  }
}

export function normalizeLocale(locale?: string | null): AppLocale | null {
  if (!locale) return null;

  const canonical = safeCanonicalizeLocale(locale)?.toLowerCase();
  if (!canonical) return null;

  const base = canonical.split("-")[0];
  return SUPPORTED_LOCALE_SET.has(base) ? (base as AppLocale) : null;
}

export function detectSystemLocale(
  locales: readonly string[] = typeof navigator === "undefined"
    ? []
    : navigator.languages,
): AppLocale {
  for (const locale of locales) {
    const normalized = normalizeLocale(locale);
    if (normalized) return normalized;
  }

  return DEFAULT_LOCALE;
}

export function getStoredLocalePreference(): AppLocale | null {
  if (typeof window === "undefined") return null;

  try {
    return normalizeLocale(window.localStorage.getItem(LOCALE_STORAGE_KEY));
  } catch {
    return null;
  }
}

export function resolveLocalePreference(
  preference: LocalePreference = SYSTEM_LOCALE,
): AppLocale {
  return preference === SYSTEM_LOCALE
    ? detectSystemLocale()
    : (normalizeLocale(preference) ?? DEFAULT_LOCALE);
}

export function getLocalePreference(): LocalePreference {
  return getStoredLocalePreference() ?? SYSTEM_LOCALE;
}

export function getInitialLocale(): AppLocale {
  return resolveLocalePreference(getLocalePreference());
}

export async function setLocalePreference(
  preference: LocalePreference,
): Promise<void> {
  if (typeof window !== "undefined") {
    try {
      if (preference === SYSTEM_LOCALE) {
        window.localStorage.removeItem(LOCALE_STORAGE_KEY);
      } else {
        window.localStorage.setItem(LOCALE_STORAGE_KEY, preference);
      }
    } catch {
      // localStorage may be unavailable
    }
  }

  const { i18n } = await import("./i18n");
  await i18n.changeLanguage(resolveLocalePreference(preference));
}

export function getCurrentLocale(): AppLocale {
  return (
    normalizeLocale(
      typeof document === "undefined" ? null : document.documentElement.lang,
    ) ??
    getStoredLocalePreference() ??
    detectSystemLocale()
  );
}

export function setDocumentLocale(locale: AppLocale): void {
  if (typeof document === "undefined") return;
  document.documentElement.lang = locale;
}

export function getLocaleDisplayName(
  locale: AppLocale,
  displayLocale: string = getCurrentLocale(),
): string {
  try {
    return (
      new Intl.DisplayNames([displayLocale], { type: "language" }).of(locale) ??
      locale
    );
  } catch {
    return locale;
  }
}
