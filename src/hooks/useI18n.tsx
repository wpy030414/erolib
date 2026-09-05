import { createContext, useContext, useState, useCallback, useEffect, useSyncExternalStore } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api } from '@/services/api';
import zh from '@/i18n/zh';
import en from '@/i18n/en';
import ja from '@/i18n/ja';

export type Locale = 'zh' | 'en' | 'ja';

export const LOCALES: Locale[] = ['zh', 'en', 'ja'];

export const LOCALE_LABELS: Record<Locale, string> = {
  zh: '中文',
  en: 'English',
  ja: '日本語',
};

const DICTS: Record<Locale, Record<string, string>> = { zh, en, ja };

// ── Locale subscription (framework-agnostic) ──────────────────────────

let currentLocale: Locale = 'zh';
const listeners = new Set<() => void>();

function detectInitialLocale(): Locale {
  if (typeof window === 'undefined') return 'zh';
  try {
    const stored = window.localStorage.getItem('erolib.locale');
    if (stored === 'zh' || stored === 'en' || stored === 'ja') return stored;
  } catch { /* ignore */ }
  const nav = navigator.language?.slice(0, 2);
  if (nav === 'en') return 'en';
  if (nav === 'ja') return 'ja';
  return 'zh';
}

currentLocale = detectInitialLocale();

function notifyListeners() {
  listeners.forEach((cb) => cb());
}

async function syncLocaleToBackend(l: Locale) {
  try {
    await api.setLocale(l);
  } catch { /* fire-and-forget */ }
  // Fire registered callbacks (e.g. library refresh)
  localeChangeCallbacks.forEach((cb) => cb());
}

// ── Public API ────────────────────────────────────────────────────────

/** Look up a translation key in the current locale. Falls back to zh, then
 *  to the raw key. Supports `{var}` placeholder substitution. */
export function t(key: string, vars?: Record<string, string | number>): string {
  const dict = DICTS[currentLocale] ?? DICTS.zh;
  let text = dict[key] ?? DICTS.zh[key] ?? key;
  if (vars) {
    text = text.replace(/\{(\w+)\}/g, (_, name) => String(vars[name] ?? `{${name}}`));
  }
  return text;
}

export function getLocale(): Locale {
  return currentLocale;
}

export async function setLocale(l: Locale): Promise<void> {
  if (currentLocale === l) return;
  currentLocale = l;
  try {
    window.localStorage.setItem('erolib.locale', l);
  } catch { /* ignore */ }
  notifyListeners();
  applyWindowTitle();
  await syncLocaleToBackend(l);
}

const localeChangeCallbacks: Array<() => void> = [];

export function onLocaleChange(cb: () => void): () => void {
  localeChangeCallbacks.push(cb);
  return () => {
    const idx = localeChangeCallbacks.indexOf(cb);
    if (idx >= 0) localeChangeCallbacks.splice(idx, 1);
  };
}

function subscribeToLocale(cb: () => void): () => void {
  listeners.add(cb);
  return () => { listeners.delete(cb); };
}

export async function applyWindowTitle(): Promise<void> {
  const title = t('app.title');
  document.title = title;
  try {
    await getCurrentWindow().setTitle(title);
  } catch { /* ignore */ }
}

// ── React hook ─────────────────────────────────────────────────────────

interface I18nContextValue {
  locale: Locale;
  setLocale: (l: Locale) => Promise<void>;
  t: typeof t;
}

const I18nContext = createContext<I18nContextValue>({
  locale: currentLocale,
  setLocale,
  t,
});

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const locale = useSyncExternalStore(subscribeToLocale, getLocale);

  const handleSetLocale = useCallback(async (l: Locale) => {
    await setLocale(l);
  }, []);

  return (
    <I18nContext.Provider value={{ locale, setLocale: handleSetLocale, t }}>
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n() {
  return useContext(I18nContext);
}