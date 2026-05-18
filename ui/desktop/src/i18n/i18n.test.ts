import { describe, it, expect, vi, afterEach } from 'vitest';
import { getLocale, loadMessagesWithCatalogLoader } from './index';

const englishMessages = {
  shared: 'English shared message',
  englishOnly: 'English fallback message',
};

const zhCnMessages = {
  shared: 'Chinese shared message',
};

// Helper to mock window.appConfig for tests
function mockAppConfig(values: Record<string, unknown>) {
  (window as unknown as Record<string, unknown>).appConfig = {
    get: (key: string) => values[key],
    getAll: () => values,
  };
}

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

  it('falls back to base language tag for message catalog', () => {
    // "en-GB" should use "en" catalog but keep "en-GB" for formatting
    vi.stubGlobal('navigator', { languages: ['en-GB'] });
    expect(getLocale()).toEqual({ locale: 'en-GB', messageLocale: 'en' });
  });

  it('falls back to base language when locale tag is invalid BCP 47', () => {
    // "en-" is not a valid BCP 47 tag and would cause RangeError in Intl APIs
    mockAppConfig({ GOOSE_LOCALE: 'en-' });
    vi.stubGlobal('navigator', { languages: ['xx-XX'] });
    expect(getLocale()).toEqual({ locale: 'en', messageLocale: 'en' });
  });
});

describe('loadMessages', () => {
  it('returns compiled English messages for English locale', async () => {
    const loadCatalog = vi.fn().mockResolvedValue(englishMessages);
    const messages = await loadMessagesWithCatalogLoader('en', loadCatalog);

    expect(messages).toEqual(englishMessages);
    expect(loadCatalog).toHaveBeenCalledOnce();
    expect(loadCatalog).toHaveBeenCalledWith('en');
  });

  it('merges locale messages over English fallback messages', async () => {
    const loadCatalog = vi.fn(async (locale: string) =>
      locale === 'zh-CN' ? zhCnMessages : englishMessages
    );
    const messages = await loadMessagesWithCatalogLoader('zh-CN', loadCatalog);

    expect(messages.shared).toBe(zhCnMessages.shared);
    expect(messages.englishOnly).toBe(englishMessages.englishOnly);
    expect(loadCatalog).toHaveBeenCalledWith('en');
    expect(loadCatalog).toHaveBeenCalledWith('zh-CN');
  });

  it('returns English messages for unsupported locale (with warning)', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const loadCatalog = vi.fn(async (locale: string) => {
      if (locale === 'xx') {
        throw new Error('missing catalog');
      }
      return englishMessages;
    });
    const messages = await loadMessagesWithCatalogLoader('xx', loadCatalog);

    expect(messages).toEqual(englishMessages);
    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining('No message catalog found'));
    warnSpy.mockRestore();
  });

  it('falls back to default messages when the English catalog is unavailable', async () => {
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const loadCatalog = vi.fn(async () => {
      throw new Error('missing catalog');
    });
    const messages = await loadMessagesWithCatalogLoader('en', loadCatalog);

    expect(messages).toEqual({});
    expect(warnSpy).toHaveBeenCalledWith(
      '[i18n] No English fallback catalog found; missing messages will use source defaultMessage values.'
    );
    warnSpy.mockRestore();
  });
});
