// Display formatters shared across views (Tasks summary, Library metadata).
// The byte / speed / duration formatters depend on i18n keys, so they take the
// `t` function as a parameter (utils stay free of Vue/composable coupling).

type TranslateFn = (key: string, vars?: Record<string, string | number>) => string;

/** Human-readable byte size without i18n, e.g. "12.3 MB" / "4 KB". Used by the
 *  Library metadata dialog (where the unit is plain text, not a templated
 *  string). Renders "—" for missing/zero. */
export function formatSize(bytes?: number): string {
  if (!bytes) return '—';
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** Byte size with i18n units (KB/MB/GB), for the Tasks summary. */
export function formatBytes(b: number, t: TranslateFn): string {
  if (b <= 0) return t('tasks.size.mb', { size: '0' });
  const mb = b / (1024 * 1024);
  if (mb >= 1024) return t('tasks.size.gb', { size: (mb / 1024).toFixed(2) });
  if (mb >= 1) return t('tasks.size.mb', { size: mb.toFixed(1) });
  return t('tasks.size.kb', { size: (b / 1024).toFixed(1) });
}

/** Download speed with i18n units (KB/s, MB/s). Persistently shown while
 *  running — 0 B/s during inter-page gaps so the readout never flickers. */
export function formatSpeed(bps: number, t: TranslateFn): string {
  if (bps <= 0) return t('tasks.speed.kbps', { speed: '0.0' });
  if (bps < 1024 * 1024) {
    return t('tasks.speed.kbps', { speed: (bps / 1024).toFixed(1) });
  }
  return t('tasks.speed.mbps', { speed: (bps / 1024 / 1024).toFixed(2) });
}

/** Elapsed milliseconds as "h时m分s秒" / "m分s秒" / "s秒" (top zero units
 *  dropped, never "0时3分20秒"). */
export function formatDuration(ms: number, t: TranslateFn): string {
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0) return t('tasks.duration.hms', { h, m, s });
  if (m > 0) return t('tasks.duration.ms', { m, s });
  return t('tasks.duration.s', { s });
}
