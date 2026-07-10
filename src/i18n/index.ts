import { ref, computed, watch } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import zh from './zh';
import en from './en';
import ja from './ja';

export type Locale = 'zh' | 'en' | 'ja';
export const LOCALES: Locale[] = ['zh', 'en', 'ja'];

export const LOCALE_LABELS: Record<Locale, string> = {
  zh: '中文',
  en: 'English',
  ja: '日本語',
};

type Dict = Record<string, string>;

const DICTIONARIES: Record<Locale, Dict> = { zh, en, ja };

const STORAGE_KEY = 'erolib.locale';

const locale = ref<Locale>(detectInitialLocale());

function detectInitialLocale(): Locale {
  if (typeof window === 'undefined') return 'zh';
  const saved = window.localStorage.getItem(STORAGE_KEY);
  if (saved && LOCALES.includes(saved as Locale)) {
    return saved as Locale;
  }
  const nav = window.navigator.language?.toLowerCase() ?? '';
  if (nav.startsWith('ja')) return 'ja';
  if (nav.startsWith('en')) return 'en';
  return 'zh';
}

export function setLocale(l: Locale) {
  locale.value = l;
  applyWindowTitle();
  try {
    window.localStorage.setItem(STORAGE_KEY, l);
  } catch {
    // ignore
  }
}

export function applyWindowTitle() {
  const title = t('app.title');
  document.title = title;
  try {
    getCurrentWindow().setTitle(title);
  } catch {
    // ignore: may fail in browser
  }
}

watch(locale, applyWindowTitle);

export function t(
  key: string,
  vars?: Record<string, string | number>,
): string {
  const dict = DICTIONARIES[locale.value] ?? zh;
  let value = dict[key] ?? zh[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      value = value.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v));
    }
  }
  return value;
}

export function useI18n() {
  return {
    locale: computed(() => locale.value),
    setLocale,
    t,
  };
}
