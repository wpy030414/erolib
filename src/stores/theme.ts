import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
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

/** Maximum number of custom themes — oldest non-active is evicted on overflow. */
const MAX_CUSTOM = 3;

export interface CustomTheme {
  key: string; // "custom:<uuid>"
  seedColorHex: string;
  /** Full-resolution source image (data URL) — used as the background overlay. */
  imageB64: string;
  /** Small thumbnail (data URL) — used in the Settings theme picker. */
  thumbnailB64: string;
  sourceBookId: string;
  sourcePage: number;
  sourceTitle: string;
  createdAt: number; // Date.now()
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
  } catch {
    return new Map();
  }
}

function saveCustomThemes(map: Map<string, CustomTheme>) {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(
      CUSTOM_THEMES_KEY,
      JSON.stringify(Object.fromEntries(map)),
    );
  } catch {
    // ignore
  }
}

export function readSavedTheme(): { seed: Seed; mode: ThemeMode } {
  if (typeof window === 'undefined') {
    return { seed: 'pink', mode: 'light' };
  }
  const savedSeed = window.localStorage.getItem(SEED_KEY) as Seed | null;
  const savedMode = window.localStorage.getItem(THEME_KEY) as ThemeMode | null;
  // Accept any saved seed value (including custom:<uuid>) — the store will
  // validate it against the built-in list and custom-themes map.
  const seed: Seed =
    savedSeed &&
    (['pink', 'violet', 'blue', 'teal'].includes(savedSeed) ||
      savedSeed.startsWith('custom:'))
      ? savedSeed
      : 'pink';
  const mode: ThemeMode = savedMode === 'dark' ? 'dark' : 'light';
  return { seed, mode };
}

export const useThemeStore = defineStore('theme', () => {
  // ── state ─────────────────────────────────────────────────────────
  const seed = ref<Seed>(
    (typeof window !== 'undefined' &&
      readSavedTheme().seed) ||
      'pink',
  );
  const mode = ref<ThemeMode>(
    (typeof window !== 'undefined' &&
      (window.localStorage.getItem(THEME_KEY) as ThemeMode | null)) ||
      (systemDark() ? 'dark' : 'light'),
  );
  const customThemes = ref<Map<string, CustomTheme>>(loadCustomThemes());

  /** Data URL of the current custom theme’s thumbnail, or null when a built-in
   *  seed is active. Used by App.vue to show/hide the background overlay. */
  const themeBgImage = ref<string | null>(null);

  // ── computed ──────────────────────────────────────────────────────
  const isCustomActive = computed(() => seed.value.startsWith('custom:'));

  // ── helpers ───────────────────────────────────────────────────────
  function persistSeed(value: Seed) {
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(SEED_KEY, value);
    }
  }

  function persistMode(value: ThemeMode) {
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(THEME_KEY, value);
    }
  }

  /** Apply the right theme engine depending on whether this is a built-in or
   *  custom seed, then sync the CSS background-image variable. */
  function applyTheme(seedValue: Seed, modeValue: ThemeMode) {
    if (seedValue.startsWith('custom:')) {
      const ct = customThemes.value.get(seedValue);
      if (ct) {
        applyArgbTheme(argbFromHex(ct.seedColorHex), modeValue === 'dark');
        document.documentElement.style.setProperty(
          '--theme-bg-image',
          `url(${ct.imageB64})`,
        );
        themeBgImage.value = ct.imageB64;
      } else {
        // Custom theme was deleted — fall back to pink.
        applyMd3Theme('pink', modeValue);
        document.documentElement.style.removeProperty('--theme-bg-image');
        themeBgImage.value = null;
      }
    } else {
      applyMd3Theme(seedValue, modeValue);
      document.documentElement.style.removeProperty('--theme-bg-image');
      themeBgImage.value = null;
    }
  }

  function persistCustomThemes() {
    saveCustomThemes(customThemes.value);
  }

  // ── public actions ────────────────────────────────────────────────

  function setSeed(value: Seed) {
    seed.value = value;
    persistSeed(value);
    applyTheme(value, mode.value);
  }

  function setMode(value: ThemeMode) {
    mode.value = value;
    persistMode(value);
    applyTheme(seed.value, value);
  }

  /** Create a new custom theme from an image-extracted colour and make it
   *  active.  Enforces the MAX_CUSTOM cap (FIFO eviction of the oldest
   *  non-active theme). */
  function addCustomTheme(
    seedHex: string,
    imageB64: string,
    thumbnailB64: string,
    page: number,
    bookId: string,
    title: string,
  ) {
    // Evict oldest non-active if at capacity.  If all three are active
    // (shouldn't happen — only one can be active at a time), evict the
    // oldest regardless.
    if (customThemes.value.size >= MAX_CUSTOM) {
      let oldestKey: string | null = null;
      let oldestTime = Infinity;
      for (const [k, v] of customThemes.value) {
        if (k === seed.value) continue; // never evict the active theme
        if (v.createdAt < oldestTime) {
          oldestTime = v.createdAt;
          oldestKey = k;
        }
      }
      // If all are active (edge case), just pick the oldest overall.
      if (!oldestKey) {
        for (const [k, v] of customThemes.value) {
          if (v.createdAt < oldestTime) {
            oldestTime = v.createdAt;
            oldestKey = k;
          }
        }
      }
      if (oldestKey) {
        customThemes.value.delete(oldestKey);
      }
    }

    // Generate a stable key — UUID v4 style random.
    const key =
      'custom:' +
      (typeof crypto !== 'undefined' && crypto.randomUUID
        ? crypto.randomUUID()
        : `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`);

    const ct: CustomTheme = {
      key,
      seedColorHex: seedHex,
      imageB64,
      thumbnailB64,
      sourceBookId: bookId,
      sourcePage: page,
      sourceTitle: title,
      createdAt: Date.now(),
    };

    customThemes.value.set(key, ct);
    persistCustomThemes();
    setSeed(key);
  }

  function activateCustomTheme(key: string) {
    setSeed(key);
  }

  function removeCustomTheme(key: string) {
    // Guard: cannot delete the currently active custom theme.
    if (seed.value === key) return;
    customThemes.value.delete(key);
    persistCustomThemes();
  }

  function activateBuiltinSeed(s: Seed) {
    setSeed(s);
  }

  // ── initialise on first store creation ────────────────────────────
  // Re-apply the active theme — the pre-mount apply in main.ts handled the
  // first paint, but the store is now live and owns the canonical "was the
  // last setSeed dark?" state.  This ensures themeBgImage and
  // --theme-bg-image are set correctly for custom seeds.
  applyTheme(seed.value, mode.value);

  return {
    seed,
    mode,
    SEEDS,
    customThemes,
    themeBgImage,
    isCustomActive,
    setSeed,
    setMode,
    addCustomTheme,
    activateCustomTheme,
    removeCustomTheme,
    activateBuiltinSeed,
  };
});
