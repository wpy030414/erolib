import { create } from 'zustand';
import {
  applyMd3Theme,
  applyArgbTheme,
  argbFromHex,
  SEEDS,
  type Seed,
  type ThemeMode,
} from '@/services/md3-theme';

const SEED_KEY = 'erolib.seed';
const THEME_KEY = 'erolib.theme';
const CUSTOM_THEMES_KEY = 'erolib.customThemes';
const MAX_CUSTOM = 3;

export interface CustomTheme {
  key: string;
  seedColorHex: string;
  imageB64: string;
  thumbnailB64: string;
  sourceBookId: string;
  sourcePage: number;
  sourceTitle: string;
  createdAt: number;
}

function systemDark(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function loadCustomThemes(): Map<string, CustomTheme> {
  if (typeof window === 'undefined') return new Map();
  try {
    const raw = window.localStorage.getItem(CUSTOM_THEMES_KEY);
    if (!raw) return new Map();
    const obj = JSON.parse(raw) as Record<string, CustomTheme>;
    return new Map(Object.entries(obj));
  } catch { return new Map(); }
}

function saveCustomThemes(map: Map<string, CustomTheme>) {
  if (typeof window === 'undefined') return;
  try { window.localStorage.setItem(CUSTOM_THEMES_KEY, JSON.stringify(Object.fromEntries(map))); } catch { /* ignore */ }
}

export function readSavedTheme(): { seed: Seed; mode: ThemeMode } {
  if (typeof window === 'undefined') return { seed: 'pink', mode: 'light' };
  const savedSeed = window.localStorage.getItem(SEED_KEY) as Seed | null;
  const savedMode = window.localStorage.getItem(THEME_KEY) as ThemeMode | null;
  const seed: Seed = savedSeed && (['pink', 'violet', 'blue', 'teal'].includes(savedSeed) || savedSeed.startsWith('custom:')) ? savedSeed : 'pink';
  // 首次启动（无存储值）默认跟随系统偏好（Vue 同款）；已存 light/dark 原样生效。
  const mode: ThemeMode = savedMode === 'dark' ? 'dark' : savedMode === 'light' ? 'light' : (systemDark() ? 'dark' : 'light');
  return { seed, mode };
}

function persistSeed(value: Seed) { if (typeof window !== 'undefined') window.localStorage.setItem(SEED_KEY, value); }
function persistMode(value: ThemeMode) { if (typeof window !== 'undefined') window.localStorage.setItem(THEME_KEY, value); }

function applyTheme(seedValue: Seed, modeValue: ThemeMode, customThemes: Map<string, CustomTheme>): string | null {
  if (seedValue.startsWith('custom:')) {
    const ct = customThemes.get(seedValue);
    if (ct) {
      applyArgbTheme(argbFromHex(ct.seedColorHex), modeValue === 'dark');
      document.documentElement.style.setProperty('--theme-bg-image', `url(${ct.imageB64})`);
      return ct.imageB64;
    } else {
      applyMd3Theme('pink', modeValue);
      document.documentElement.style.removeProperty('--theme-bg-image');
      return null;
    }
  } else {
    applyMd3Theme(seedValue, modeValue);
    document.documentElement.style.removeProperty('--theme-bg-image');
    return null;
  }
}

interface ThemeState {
  seed: Seed;
  mode: ThemeMode;
  SEEDS: typeof SEEDS;
  customThemes: Map<string, CustomTheme>;
  themeBgImage: string | null;
  isCustomActive: boolean;
  setSeed: (value: Seed) => void;
  setMode: (value: ThemeMode) => void;
  addCustomTheme: (seedHex: string, imageB64: string, thumbnailB64: string, page: number, bookId: string, title: string) => void;
  activateCustomTheme: (key: string) => void;
  removeCustomTheme: (key: string) => void;
  activateBuiltinSeed: (s: Seed) => void;
}

const initialTheme = readSavedTheme();

export const useThemeStore = create<ThemeState>((set, get) => {
  const customThemes = loadCustomThemes();

  return {
    seed: initialTheme.seed,
    mode: initialTheme.mode,
    SEEDS,
    customThemes,
    themeBgImage: null,
    isCustomActive: initialTheme.seed.startsWith('custom:'),

    setSeed: (value) => {
      persistSeed(value);
      const isCustom = value.startsWith('custom:');
      const bgImage = applyTheme(value, get().mode, get().customThemes);
      set({ seed: value, isCustomActive: isCustom, themeBgImage: bgImage });
    },

    setMode: (value) => {
      persistMode(value);
      const bgImage = applyTheme(get().seed, value, get().customThemes);
      set({ mode: value, themeBgImage: bgImage });
    },

    addCustomTheme: (seedHex, imageB64, thumbnailB64, page, bookId, title) => {
      const current = get();
      const ct = current.customThemes;
      if (ct.size >= MAX_CUSTOM) {
        let oldestKey: string | null = null;
        let oldestTime = Infinity;
        for (const [k, v] of ct) {
          if (k === current.seed) continue;
          if (v.createdAt < oldestTime) { oldestTime = v.createdAt; oldestKey = k; }
        }
        if (!oldestKey) {
          for (const [k, v] of ct) {
            if (v.createdAt < oldestTime) { oldestTime = v.createdAt; oldestKey = k; }
          }
        }
        if (oldestKey) ct.delete(oldestKey);
      }
      const key = 'custom:' + (typeof crypto !== 'undefined' && crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`);
      const newCT: CustomTheme = { key, seedColorHex: seedHex, imageB64, thumbnailB64, sourceBookId: bookId, sourcePage: page, sourceTitle: title, createdAt: Date.now() };
      ct.set(key, newCT);
      saveCustomThemes(ct);
      set({ customThemes: new Map(ct) });
      get().setSeed(key);
    },

    activateCustomTheme: (key) => { get().setSeed(key); },

    removeCustomTheme: (key) => {
      const current = get();
      if (current.seed === key) return;
      const ct = current.customThemes;
      ct.delete(key);
      saveCustomThemes(ct);
      set({ customThemes: new Map(ct) });
    },

    activateBuiltinSeed: (s) => { get().setSeed(s); },
  };
});

// Apply the persisted theme at module load (pre-mount), and write the
// resulting custom-wallpaper state back into the store so the overlay
// renders right after a restart with a custom theme active.
const initialBgImage = applyTheme(initialTheme.seed, initialTheme.mode, useThemeStore.getState().customThemes);
useThemeStore.setState({ themeBgImage: initialBgImage });