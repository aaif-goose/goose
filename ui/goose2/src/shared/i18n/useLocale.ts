import { useTranslation } from "react-i18next";

import { SYSTEM_LOCALE, type LocalePreference } from "./constants";
import {
  detectSystemLocale,
  getCurrentLocale,
  getLocaleDisplayName,
  getLocalePreference,
  setLocalePreference,
} from "./locale";

export function useLocale() {
  const { i18n } = useTranslation();
  const locale = i18n.resolvedLanguage ?? getCurrentLocale();
  const preference = getLocalePreference();
  const systemLocale = detectSystemLocale();

  return {
    locale,
    preference,
    setLocalePreference: (nextPreference: LocalePreference) =>
      setLocalePreference(nextPreference),
    systemLocaleLabel: getLocaleDisplayName(systemLocale, locale),
    isSystemLocale: preference === SYSTEM_LOCALE,
  };
}
