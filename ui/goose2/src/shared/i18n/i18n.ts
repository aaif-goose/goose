import i18next from "i18next";
import resourcesToBackend from "i18next-resources-to-backend";
import { initReactI18next } from "react-i18next";

import {
  DEFAULT_LOCALE,
  DEFAULT_NAMESPACE,
  SUPPORTED_LOCALES,
  type AppLocale,
  type TranslationNamespace,
} from "./constants";
import { getInitialLocale } from "./locale";

function normalizeSupportedLocale(locale?: string | null): AppLocale | null {
  if (!locale) return null;

  try {
    const canonical = Intl.getCanonicalLocales(locale)[0]?.toLowerCase();
    if (!canonical) return null;
    const base = canonical.split("-")[0];
    return SUPPORTED_LOCALES.includes(base as AppLocale)
      ? (base as AppLocale)
      : null;
  } catch {
    return null;
  }
}

const localeResourceLoaders = {
  en: {
    agents: () => import("./locales/en/agents.json"),
    common: () => import("./locales/en/common.json"),
    chat: () => import("./locales/en/chat.json"),
    home: () => import("./locales/en/home.json"),
    projects: () => import("./locales/en/projects.json"),
    settings: () => import("./locales/en/settings.json"),
    skills: () => import("./locales/en/skills.json"),
    sidebar: () => import("./locales/en/sidebar.json"),
    status: () => import("./locales/en/status.json"),
    sessions: () => import("./locales/en/sessions.json"),
  },
  es: {
    agents: () => import("./locales/es/agents.json"),
    common: () => import("./locales/es/common.json"),
    chat: () => import("./locales/es/chat.json"),
    home: () => import("./locales/es/home.json"),
    projects: () => import("./locales/es/projects.json"),
    settings: () => import("./locales/es/settings.json"),
    skills: () => import("./locales/es/skills.json"),
    sidebar: () => import("./locales/es/sidebar.json"),
    status: () => import("./locales/es/status.json"),
    sessions: () => import("./locales/es/sessions.json"),
  },
} as const satisfies Record<
  AppLocale,
  Record<TranslationNamespace, () => Promise<unknown>>
>;

function getNamespaceLoader(language: string, namespace: string) {
  const locale = normalizeSupportedLocale(language) ?? DEFAULT_LOCALE;
  const typedNamespace = namespace as TranslationNamespace;
  return localeResourceLoaders[locale][typedNamespace];
}

export const i18n = i18next.createInstance();

void i18n
  .use(
    resourcesToBackend((language: string, namespace: string) =>
      getNamespaceLoader(language, namespace)(),
    ),
  )
  .use(initReactI18next)
  .init({
    lng: getInitialLocale(),
    fallbackLng: DEFAULT_LOCALE,
    supportedLngs: SUPPORTED_LOCALES,
    defaultNS: DEFAULT_NAMESPACE,
    fallbackNS: DEFAULT_NAMESPACE,
    ns: [DEFAULT_NAMESPACE],
    interpolation: {
      escapeValue: false,
    },
    react: {
      useSuspense: false,
    },
    returnEmptyString: false,
  });
