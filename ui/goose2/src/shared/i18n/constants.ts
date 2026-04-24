export const DEFAULT_LOCALE = "en";
export const SUPPORTED_LOCALES = ["en", "es"] as const;
export const DEFAULT_NAMESPACE = "common";
export const TRANSLATION_NAMESPACES = [
  "agents",
  "common",
  "chat",
  "home",
  "projects",
  "settings",
  "skills",
  "sidebar",
  "status",
  "sessions",
] as const;
export const LOCALE_STORAGE_KEY = "goose:locale";
export const SYSTEM_LOCALE = "system";

export type AppLocale = (typeof SUPPORTED_LOCALES)[number];
export type LocalePreference = AppLocale | typeof SYSTEM_LOCALE;
export type TranslationNamespace = (typeof TRANSLATION_NAMESPACES)[number];
