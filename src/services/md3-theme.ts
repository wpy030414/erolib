import {
  themeFromSourceColor,
  argbFromHex,
  hexFromArgb,
  type Theme,
  type TonalPalette,
} from '@material/material-color-utilities';

export type Seed = 'pink' | 'violet' | 'blue' | 'teal';
export type ThemeMode = 'light' | 'dark';

/** Available seed colors shown in Settings. */
export const SEEDS: Array<{ key: Seed; color: string }> = [
  { key: 'pink', color: '#ab2a72' },
  { key: 'violet', color: '#8320c0' },
  { key: 'blue', color: '#204cd0' },
  { key: 'teal', color: '#00605c' },
];

export function themeName(seed: Seed, mode: ThemeMode): string {
  return `${seed}${mode === 'dark' ? 'Dark' : 'Light'}`;
}

const themeCache = new Map<Seed, Theme>();

function getTheme(seed: Seed): Theme {
  const cached = themeCache.get(seed);
  if (cached) return cached;
  const seedColor = SEEDS.find((s) => s.key === seed)?.color ?? '#ab2a72';
  const theme = themeFromSourceColor(argbFromHex(seedColor));
  themeCache.set(seed, theme);
  return theme;
}

/** hex of a tonal-palette tone — used for the MD3 surface-container roles. */
function toneHex(palette: TonalPalette, tone: number): string {
  return hexFromArgb(palette.tone(tone));
}

/** MD3 surface container roles derived from the neutral palette. */
function surfaceRoles(theme: Theme, dark: boolean): Record<string, string> {
  const n = theme.palettes.neutral;
  return dark
    ? {
        'surface-dim': toneHex(n, 6),
        'surface-bright': toneHex(n, 24),
        'surface-container-lowest': toneHex(n, 4),
        'surface-container-low': toneHex(n, 10),
        'surface-container': toneHex(n, 12),
        'surface-container-high': toneHex(n, 17),
        'surface-container-highest': toneHex(n, 22),
      }
    : {
        'surface-dim': toneHex(n, 87),
        'surface-bright': toneHex(n, 98),
        'surface-container-lowest': toneHex(n, 100),
        'surface-container-low': toneHex(n, 96),
        'surface-container': toneHex(n, 94),
        'surface-container-high': toneHex(n, 92),
        'surface-container-highest': toneHex(n, 90),
      };
}

/**
 * Success / warning / info are not part of the MD3 color-role spec and
 * @material/material-color-utilities does not generate them. We supply a
 * fixed, contrast-safe pair per mode.
 */
const SEMANTIC_LIGHT: Record<string, string> = {
  success: '#386a20',
  'on-success': '#ffffff',
  'success-container': '#b7f397',
  'on-success-container': '#042100',
  warning: '#7c5800',
  'on-warning': '#ffffff',
  'warning-container': '#ffdea6',
  'on-warning-container': '#271900',
  info: '#00639b',
  'on-info': '#ffffff',
  'info-container': '#cee5ff',
  'on-info-container': '#001d34',
};

const SEMANTIC_DARK: Record<string, string> = {
  success: '#9cd67d',
  'on-success': '#072100',
  'success-container': '#1f4e08',
  'on-success-container': '#b7f397',
  warning: '#f6bd48',
  'on-warning': '#412d00',
  'warning-container': '#5d4200',
  'on-warning-container': '#ffdea6',
  info: '#9bcaff',
  'on-info': '#003355',
  'info-container': '#004a77',
  'on-info-container': '#cee5ff',
};

/**
 * Regenerate the full MD3 color scheme from the seed + mode and write every
 * `--md-sys-color-*` token onto :root. All components read these tokens, so a
 * single call here restyles the whole app.
 */
export function applyMd3Theme(seed: Seed, mode: ThemeMode): void {
  if (typeof document === 'undefined') return;
  const dark = mode === 'dark';
  const theme = getTheme(seed);
  const scheme = dark ? theme.schemes.dark : theme.schemes.light;
  const root = document.documentElement;

  const set = (token: string, value: string) =>
    root.style.setProperty(`--md-sys-color-${token}`, value);

  // Core roles from the HCT-derived scheme.
  set('primary', hexFromArgb(scheme.primary));
  set('on-primary', hexFromArgb(scheme.onPrimary));
  set('primary-container', hexFromArgb(scheme.primaryContainer));
  set('on-primary-container', hexFromArgb(scheme.onPrimaryContainer));
  set('secondary', hexFromArgb(scheme.secondary));
  set('on-secondary', hexFromArgb(scheme.onSecondary));
  set('secondary-container', hexFromArgb(scheme.secondaryContainer));
  set('on-secondary-container', hexFromArgb(scheme.onSecondaryContainer));
  set('tertiary', hexFromArgb(scheme.tertiary));
  set('on-tertiary', hexFromArgb(scheme.onTertiary));
  set('tertiary-container', hexFromArgb(scheme.tertiaryContainer));
  set('on-tertiary-container', hexFromArgb(scheme.onTertiaryContainer));
  set('error', hexFromArgb(scheme.error));
  set('on-error', hexFromArgb(scheme.onError));
  set('error-container', hexFromArgb(scheme.errorContainer));
  set('on-error-container', hexFromArgb(scheme.onErrorContainer));
  set('background', hexFromArgb(scheme.background));
  set('on-background', hexFromArgb(scheme.onBackground));
  set('surface', hexFromArgb(scheme.surface));
  set('on-surface', hexFromArgb(scheme.onSurface));
  set('surface-variant', hexFromArgb(scheme.surfaceVariant));
  set('on-surface-variant', hexFromArgb(scheme.onSurfaceVariant));
  set('outline', hexFromArgb(scheme.outline));
  set('outline-variant', hexFromArgb(scheme.outlineVariant));
  set('inverse-surface', hexFromArgb(scheme.inverseSurface));
  set('inverse-on-surface', hexFromArgb(scheme.inverseOnSurface));
  set('inverse-primary', hexFromArgb(scheme.inversePrimary));
  set('shadow', hexFromArgb(scheme.shadow));
  set('scrim', hexFromArgb(scheme.scrim));

  // Surface containers from the neutral palette (key MD3 depth cue).
  for (const [token, value] of Object.entries(surfaceRoles(theme, dark))) {
    set(token, value);
  }

  // Non-spec semantic colors.
  for (const [token, value] of Object.entries(dark ? SEMANTIC_DARK : SEMANTIC_LIGHT)) {
    set(token, value);
  }

  root.setAttribute('data-md-mode', mode);
  root.setAttribute('data-md-seed', seed);
}
