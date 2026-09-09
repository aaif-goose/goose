export type ExternalLinkLabels = {
  title: string;
  message: string;
  detail: string;
  open: string;
  cancel: string;
};

const labelsByLocale: Record<string, ExternalLinkLabels> = {
  en: {
    title: 'Open External Link',
    message: 'Open {protocol} link?',
    detail: 'This will open: {href}',
    open: 'Open',
    cancel: 'Cancel',
  },
  de: {
    title: 'Externen Link öffnen',
    message: '{protocol}-Link öffnen?',
    detail: 'Dies öffnet: {href}',
    open: 'Öffnen',
    cancel: 'Abbrechen',
  },
  es: {
    title: 'Abrir enlace externo',
    message: '¿Abrir enlace {protocol}?',
    detail: 'Esto abrirá: {href}',
    open: 'Abrir',
    cancel: 'Cancelar',
  },
  fr: {
    title: 'Ouvrir le lien externe',
    message: 'Ouvrir le lien {protocol} ?',
    detail: 'Cela ouvrira : {href}',
    open: 'Ouvrir',
    cancel: 'Annuler',
  },
  hi: {
    title: 'बाहरी लिंक खोलें',
    message: '{protocol} लिंक खोलें?',
    detail: 'यह खुलेगा: {href}',
    open: 'खुला',
    cancel: 'रद्द करें',
  },
  id: {
    title: 'Buka Tautan Eksternal',
    message: 'Buka tautan {protocol}?',
    detail: 'Ini akan membuka: {href}',
    open: 'Buka',
    cancel: 'Batal',
  },
  it: {
    title: 'Apri link esterno',
    message: 'Aprire il link {protocol}?',
    detail: 'Questo aprirà: {href}',
    open: 'Apri',
    cancel: 'Annulla',
  },
  ja: {
    title: '外部リンクを開く',
    message: '{protocol}リンクを開きますか？',
    detail: '次を開きます: {href}',
    open: '開く',
    cancel: 'キャンセル',
  },
  ko: {
    title: '외부 링크 열기',
    message: '{protocol} 링크를 열까요?',
    detail: '열립니다: {href}',
    open: '열기',
    cancel: '취소',
  },
  ms: {
    title: 'Buka Pautan Luaran',
    message: 'Buka pautan {protocol}?',
    detail: 'Ini akan membuka: {href}',
    open: 'Buka',
    cancel: 'Batal',
  },
  pt: {
    title: 'Abrir Link Externo',
    message: 'Abrir link {protocol}?',
    detail: 'Isto abrirá: {href}',
    open: 'Abrir',
    cancel: 'Cancelar',
  },
  ru: {
    title: 'Открыть внешнюю ссылку',
    message: 'Открыть ссылку {protocol}?',
    detail: 'Будет открыто: {href}',
    open: 'Открыть',
    cancel: 'Отмена',
  },
  tr: {
    title: 'Dış Bağlantıyı Aç',
    message: '{protocol} bağlantısı açılsın mı?',
    detail: 'Bu açılacak: {href}',
    open: 'Açık',
    cancel: 'İptal',
  },
  vi: {
    title: 'Mở liên kết bên ngoài',
    message: 'Mở liên kết {protocol}?',
    detail: 'Thao tác này sẽ mở: {href}',
    open: 'Mở',
    cancel: 'Hủy',
  },
  'zh-CN': {
    title: '打开外部链接',
    message: '打开 {protocol} 链接？',
    detail: '这将打开：{href}',
    open: '打开',
    cancel: '取消',
  },
  'zh-TW': {
    title: '開啟外部連結',
    message: '要開啟 {protocol} 連結嗎？',
    detail: '這會開啟：{href}',
    open: '開啟',
    cancel: '取消',
  },
};

const selectLocale = (locale?: string): string => {
  const normalized = locale?.replace(/_/g, '-') ?? 'en';
  if (labelsByLocale[normalized]) return normalized;

  const lower = normalized.toLowerCase();
  if (lower === 'zh' || lower.startsWith('zh-')) {
    return /^zh-(hant|tw|hk|mo)\b/.test(lower) ? 'zh-TW' : 'zh-CN';
  }

  const language = lower.split('-')[0];
  return labelsByLocale[language] ? language : 'en';
};

export const getExternalLinkLabels = (locale?: string): ExternalLinkLabels =>
  labelsByLocale[selectLocale(locale)];
