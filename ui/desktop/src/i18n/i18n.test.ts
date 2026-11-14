import { describe, it, expect, vi, afterEach } from 'vitest';
import { getLocale, normalizeLocaleTag } from './index';
import { formatClockDisplay } from '../utils/timeUtils';

// Helper to mock window.appConfig for tests
function mockAppConfig(values: Record<string, unknown>) {
  (window as unknown as Record<string, unknown>).appConfig = {
    get: (key: string) => values[key],
    getAll: () => values,
  };
}

describe('normalizeLocaleTag', () => {
  it.each([
    ['en-US@rg=nozzzz', 'en-NO'],
    ['en_US@rg=nozzzz', 'en-NO'],
    ['nb_NO@rg=nozzzz', 'nb-NO'],
    ['nn_NO@rg=nozzzz', 'nn-NO'],
    ['zh_Hant_TW@rg=hkzzzz', 'zh-Hant-HK'],
    ['sr-Latn@rg=rszzzz', 'sr-Latn-RS'],
    ['es-ES@rg=419zzzz', 'es-419'],
    ['en-US@calendar=gregorian;rg=nozzzz', 'en-NO'],
    ['en-US-u-hc-h12@rg=nozzzz', 'en-NO-u-hc-h12'],
  ])('applies the region override in %s as %s', (input, expected) => {
    expect(normalizeLocaleTag(input)).toBe(expected);
  });

  it.each([
    'en',
    'en-US',
    'en-GB',
    'nb-NO',
    'nn-NO',
    'zh-Hant-TW',
    'sr-Latn-RS',
    'de-DE-u-co-phonebk',
  ])('preserves ordinary locale tag %s', (tag) => {
    expect(normalizeLocaleTag(tag)).toBe(tag);
  });

  it.each([
    ['ko_KR', 'ko-KR'],
    ['de-DE@calendar=gregorian', 'de-DE'],
    ['en-US@rg=invalid', 'en-US'],
    ['en-@rg=nozzzz', 'en-'],
  ])('normalizes %s without a valid region override to %s', (input, expected) => {
    expect(normalizeLocaleTag(input)).toBe(expected);
  });
});

describe('getLocale', () => {
  afterEach(() => {
    // Clean up appConfig mock
    if (typeof window !== 'undefined') {
      delete (window as unknown as Record<string, unknown>).appConfig;
    }
    vi.restoreAllMocks();
  });

  it('returns "en" as the default fallback', () => {
    // navigator.languages contains only unsupported tags
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'en', messageLocale: 'en' });
  });

  it('preserves regional tag for formatting when base language is supported', () => {
    vi.stubGlobal('navigator', { languages: ['en-US'] });
    expect(getLocale()).toEqual({ locale: 'en-US', messageLocale: 'en' });
  });

  it('returns exact match when navigator.languages contains a supported locale', () => {
    vi.stubGlobal('navigator', { languages: ['en'] });
    expect(getLocale()).toEqual({ locale: 'en', messageLocale: 'en' });
  });

  it('respects GOOSE_LOCALE over navigator.languages', () => {
    mockAppConfig({ GOOSE_LOCALE: 'en' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'en', messageLocale: 'en' });
  });

  it('preserves regional tag from GOOSE_LOCALE', () => {
    mockAppConfig({ GOOSE_LOCALE: 'en-GB' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'en-GB', messageLocale: 'en' });
  });

  it('uses the macOS region override for 24-hour clock formatting', () => {
    mockAppConfig({ GOOSE_LOCALE: 'en-US@rg=nozzzz' });
    vi.stubGlobal('navigator', { languages: ['en-US'] });

    const resolved = getLocale();
    expect(resolved).toEqual({ locale: 'en-NO', messageLocale: 'en' });
    expect(formatClockDisplay(new Date(2026, 9, 14, 18, 28), resolved.locale)).toEqual({
      time: '18:28',
      meridiem: '',
      hour: 18,
    });
  });

  it('preserves the script subtag when resolving a region override', () => {
    mockAppConfig({ GOOSE_LOCALE: 'ru-Cyrl-RU@rg=byzzzz' });
    vi.stubGlobal('navigator', { languages: ['en-US'] });
    expect(getLocale()).toEqual({ locale: 'ru-Cyrl-BY', messageLocale: 'ru' });
  });

  it('normalizes a region override from navigator.languages', () => {
    vi.stubGlobal('navigator', { languages: ['en-US@rg=nozzzz'] });
    expect(getLocale()).toEqual({ locale: 'en-NO', messageLocale: 'en' });
  });

  it.each([
    ['zh_CN@rg=hkzzzz', 'zh-HK', 'zh-CN'],
    ['zh_CN@rg=mozzzz', 'zh-MO', 'zh-CN'],
    ['zh_CN@rg=twzzzz', 'zh-TW', 'zh-CN'],
    ['zh_Hans_CN@rg=hkzzzz', 'zh-Hans-HK', 'zh-CN'],
    ['zh_Hant_TW@rg=cnzzzz', 'zh-Hant-CN', 'zh-TW'],
    ['zh_Hant_TW@rg=twzzzz', 'zh-Hant-TW', 'zh-TW'],
    ['zh_TW@rg=cnzzzz', 'zh-CN', 'zh-TW'],
    ['zh_HK@rg=cnzzzz', 'zh-CN', 'zh-TW'],
  ])(
    'keeps the Chinese catalog when applying the region override in %s',
    (tag, locale, messageLocale) => {
      mockAppConfig({ GOOSE_LOCALE: tag });
      vi.stubGlobal('navigator', { languages: ['xx-XX'] });
      expect(getLocale()).toEqual({ locale, messageLocale });
    }
  );

  it('keeps the Chinese catalog for a region override from navigator.languages', () => {
    vi.stubGlobal('navigator', { languages: ['zh_CN@rg=hkzzzz'] });
    expect(getLocale()).toEqual({ locale: 'zh-HK', messageLocale: 'zh-CN' });
  });

  it.each([
    ['en', 'en', 'en'],
    ['en-US', 'en-US', 'en'],
    ['en-GB', 'en-GB', 'en'],
    ['nb-NO', 'en', 'en'],
    ['nn-NO', 'en', 'en'],
    ['ru-Cyrl-RU', 'ru-Cyrl-RU', 'ru'],
    ['zh-CN', 'zh-CN', 'zh-CN'],
    ['zh-Hans-CN', 'zh-CN', 'zh-CN'],
    ['zh-Hant-TW', 'zh-TW', 'zh-TW'],
    ['zh-TW', 'zh-TW', 'zh-TW'],
    ['zh-HK', 'zh-TW', 'zh-TW'],
    ['zh-MO', 'zh-TW', 'zh-TW'],
    ['de-DE-u-co-phonebk', 'de-DE-u-co-phonebk', 'de'],
  ])('keeps existing locale and catalog resolution for %s', (tag, locale, messageLocale) => {
    mockAppConfig({ GOOSE_LOCALE: tag });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale, messageLocale });
  });

  it('falls back to base language tag for message catalog', () => {
    // "en-GB" should use "en" catalog but keep "en-GB" for formatting
    vi.stubGlobal('navigator', { languages: ['en-GB'] });
    expect(getLocale()).toEqual({ locale: 'en-GB', messageLocale: 'en' });
  });

  it('returns Russian when navigator.languages contains ru', () => {
    vi.stubGlobal('navigator', { languages: ['ru'] });
    expect(getLocale()).toEqual({ locale: 'ru', messageLocale: 'ru' });
  });

  it('preserves Russian regional tag for formatting', () => {
    vi.stubGlobal('navigator', { languages: ['ru-RU'] });
    expect(getLocale()).toEqual({ locale: 'ru-RU', messageLocale: 'ru' });
  });

  it('supports Turkish from navigator.languages', () => {
    vi.stubGlobal('navigator', { languages: ['tr-TR'] });
    expect(getLocale()).toEqual({ locale: 'tr-TR', messageLocale: 'tr' });
  });

  it('supports explicit Turkish locale', () => {
    mockAppConfig({ GOOSE_LOCALE: 'tr' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'tr', messageLocale: 'tr' });
  });

  it('supports Korean from navigator.languages', () => {
    vi.stubGlobal('navigator', { languages: ['ko-KR'] });
    expect(getLocale()).toEqual({ locale: 'ko-KR', messageLocale: 'ko' });
  });

  it('supports explicit Korean locale', () => {
    mockAppConfig({ GOOSE_LOCALE: 'ko' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'ko', messageLocale: 'ko' });
  });

  it('supports POSIX-style Korean locale from GOOSE_LOCALE', () => {
    mockAppConfig({ GOOSE_LOCALE: 'ko_KR' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'ko-KR', messageLocale: 'ko' });
  });

  it('supports Japanese from navigator.languages', () => {
    vi.stubGlobal('navigator', { languages: ['ja-JP'] });
    expect(getLocale()).toEqual({ locale: 'ja-JP', messageLocale: 'ja' });
  });

  it('supports POSIX-style Japanese locale from GOOSE_LOCALE', () => {
    mockAppConfig({ GOOSE_LOCALE: 'ja_JP' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'ja-JP', messageLocale: 'ja' });
  });

  it('supports Hindi from navigator.languages', () => {
    vi.stubGlobal('navigator', { languages: ['hi-IN'] });
    expect(getLocale()).toEqual({ locale: 'hi-IN', messageLocale: 'hi' });
  });

  it('supports explicit Hindi locale', () => {
    mockAppConfig({ GOOSE_LOCALE: 'hi' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'hi', messageLocale: 'hi' });
  });

  it('supports Spanish from navigator.languages', () => {
    vi.stubGlobal('navigator', { languages: ['es-ES'] });
    expect(getLocale()).toEqual({ locale: 'es-ES', messageLocale: 'es' });
  });

  it('supports explicit Spanish locale', () => {
    mockAppConfig({ GOOSE_LOCALE: 'es' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'es', messageLocale: 'es' });
  });

  it('falls back to base language when locale tag is invalid BCP 47', () => {
    // "en-" is not a valid BCP 47 tag and would cause RangeError in Intl APIs
    mockAppConfig({ GOOSE_LOCALE: 'en-' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'en', messageLocale: 'en' });
  });
});

describe('loadMessages', () => {
  it('returns empty object for English locale', async () => {
    const { loadMessages } = await import('./index');
    const messages = await loadMessages('en');
    expect(messages).toEqual({});
  });

  it('returns empty object for unsupported locale (with warning)', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { loadMessages } = await import('./index');
    const messages = await loadMessages('xx');
    expect(messages).toEqual({});
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('No message catalog found'));
    warnSpy.mockRestore();
  });
});
