import { ref, computed, watch } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api } from '@/services/api';
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
  syncLocaleToBackend(l);
}

/** Push the locale to the backend so SQL renders tags in the current language,
 *  then fire any registered refresh callbacks (e.g. re-pull the library so the
 *  grid + chips + metadata re-render in the new language). Fire-and-forget:
 *  a failed push must not break the UI language switch. */
function syncLocaleToBackend(l: Locale) {
  void api.setLocale(l).catch(() => {});
  for (const cb of localeChangeCallbacks) {
    try {
      cb(l);
    } catch {
      // a misbehaving listener must not break others
    }
  }
}

const localeChangeCallbacks: Array<(l: Locale) => void> = [];

/** Register a callback run after the locale is pushed to the backend (used by
 *  the app shell to refresh tag-bearing views without i18n importing stores —
 *  which would be a circular dependency). */
export function onLocaleChange(cb: (l: Locale) => void) {
  localeChangeCallbacks.push(cb);
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
