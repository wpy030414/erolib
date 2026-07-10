import { defineStore } from 'pinia';
import { ref } from 'vue';
import {
  applyMd3Theme,
  SEEDS,
  themeName,
  type Seed,
  type ThemeMode,
} from '@/services/md3-theme';

const SEED_KEY = 'erolib.seed';
const THEME_KEY = 'erolib.theme';

function systemDark(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

export function readSavedTheme(): { seed: Seed; mode: ThemeMode } {
  if (typeof window === 'undefined') {
    return { seed: 'pink', mode: 'light' };
  }
  const savedSeed = window.localStorage.getItem(SEED_KEY) as Seed | null;
  const savedMode = window.localStorage.getItem(THEME_KEY) as ThemeMode | null;
  const seed =
    savedSeed && ['pink', 'violet', 'blue', 'teal'].includes(savedSeed)
      ? savedSeed
      : 'pink';
  const mode: ThemeMode = savedMode === 'dark' ? 'dark' : 'light';
  return { seed, mode };
}

export const useThemeStore = defineStore('theme', () => {
  const seed = ref<Seed>(
    (typeof window !== 'undefined' &&
      (window.localStorage.getItem(SEED_KEY) as Seed | null)) ||
      'pink',
  );
  const mode = ref<ThemeMode>(
    (typeof window !== 'undefined' &&
      (window.localStorage.getItem(THEME_KEY) as ThemeMode | null)) ||
      (systemDark() ? 'dark' : 'light'),
  );

  function currentThemeName(): string {
    return themeName(seed.value, mode.value);
  }

  function setSeed(value: Seed) {
    seed.value = value;
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(SEED_KEY, value);
    }
    applyMd3Theme(seed.value, mode.value);
  }

  function setMode(value: ThemeMode) {
    mode.value = value;
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(THEME_KEY, value);
    }
    applyMd3Theme(seed.value, mode.value);
  }

  return {
    seed,
    mode,
    SEEDS,
    setSeed,
    setMode,
  };
});
