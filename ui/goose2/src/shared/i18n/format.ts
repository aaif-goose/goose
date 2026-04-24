import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import { getCurrentLocale } from "./locale";

function toDate(value: Date | string | number): Date {
  return value instanceof Date ? value : new Date(value);
}

export function formatNumber(
  value: number,
  options?: Intl.NumberFormatOptions,
  locale: string = getCurrentLocale(),
): string {
  return new Intl.NumberFormat(locale, options).format(value);
}

export function formatDate(
  value: Date | string | number,
  options?: Intl.DateTimeFormatOptions,
  locale: string = getCurrentLocale(),
): string {
  return new Intl.DateTimeFormat(locale, options).format(toDate(value));
}

export function formatDateParts(
  value: Date | string | number,
  options?: Intl.DateTimeFormatOptions,
  locale: string = getCurrentLocale(),
): Intl.DateTimeFormatPart[] {
  return new Intl.DateTimeFormat(locale, options).formatToParts(toDate(value));
}

export function formatRelativeTime(
  value: number,
  unit: Intl.RelativeTimeFormatUnit,
  options?: Intl.RelativeTimeFormatOptions,
  locale: string = getCurrentLocale(),
): string {
  return new Intl.RelativeTimeFormat(locale, options).format(value, unit);
}

export function formatRelativeTimeToNow(
  value: Date | string | number,
  locale: string = getCurrentLocale(),
  options: Intl.RelativeTimeFormatOptions = {
    numeric: "auto",
    style: "short",
  },
): string {
  const date = toDate(value);
  const diffMs = date.getTime() - Date.now();
  const diffSeconds = Math.round(diffMs / 1000);

  if (Math.abs(diffSeconds) < 45) {
    return formatRelativeTime(0, "second", options, locale);
  }

  const diffMinutes = Math.round(diffMs / 60_000);
  if (Math.abs(diffMinutes) < 45) {
    return formatRelativeTime(diffMinutes, "minute", options, locale);
  }

  const diffHours = Math.round(diffMs / 3_600_000);
  if (Math.abs(diffHours) < 24) {
    return formatRelativeTime(diffHours, "hour", options, locale);
  }

  const diffDays = Math.round(diffMs / 86_400_000);
  if (Math.abs(diffDays) < 7) {
    return formatRelativeTime(diffDays, "day", options, locale);
  }

  return formatDate(
    date,
    {
      month: "short",
      day: "numeric",
    },
    locale,
  );
}

export function getTimeParts(
  value: Date | string | number,
  locale: string = getCurrentLocale(),
  options: Intl.DateTimeFormatOptions = {
    hour: "numeric",
    minute: "2-digit",
  },
): {
  hour: string;
  minute: string;
  dayPeriod?: string;
} {
  const parts = formatDateParts(value, options, locale);

  return {
    hour: parts.find((part) => part.type === "hour")?.value ?? "",
    minute: parts.find((part) => part.type === "minute")?.value ?? "",
    dayPeriod: parts.find((part) => part.type === "dayPeriod")?.value,
  };
}

export function useLocaleFormatting() {
  const { i18n } = useTranslation();
  const locale = i18n.resolvedLanguage ?? getCurrentLocale();

  return useMemo(
    () => ({
      locale,
      formatNumber: (value: number, options?: Intl.NumberFormatOptions) =>
        formatNumber(value, options, locale),
      formatDate: (
        value: Date | string | number,
        options?: Intl.DateTimeFormatOptions,
      ) => formatDate(value, options, locale),
      formatDateParts: (
        value: Date | string | number,
        options?: Intl.DateTimeFormatOptions,
      ) => formatDateParts(value, options, locale),
      formatRelativeTime: (
        value: number,
        unit: Intl.RelativeTimeFormatUnit,
        options?: Intl.RelativeTimeFormatOptions,
      ) => formatRelativeTime(value, unit, options, locale),
      formatRelativeTimeToNow: (
        value: Date | string | number,
        options?: Intl.RelativeTimeFormatOptions,
      ) => formatRelativeTimeToNow(value, locale, options),
      getTimeParts: (
        value: Date | string | number,
        options?: Intl.DateTimeFormatOptions,
      ) => getTimeParts(value, locale, options),
    }),
    [locale],
  );
}
