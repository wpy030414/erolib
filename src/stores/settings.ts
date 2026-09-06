import { create } from 'zustand';
import { api } from '@/services/api';

const OPDS_PORT_KEY = 'erolib.opdsPort';
const RSS_PORT_KEY = 'erolib.rssPort';
const SYNC_ENABLED_KEY = 'erolib.localSyncEnabled';
const SYNC_DIR_KEY = 'erolib.localSyncDir';

function loadVal(key: string, fallback: string): string {
  if (typeof window === 'undefined') return fallback;
  try { return window.localStorage.getItem(key) ?? fallback; } catch { return fallback; }
}
function loadPort(key: string, fallback: string): string {
  if (typeof window === 'undefined') return fallback;
  try {
    const raw = window.localStorage.getItem(key);
    if (raw === null) return fallback;
    // Stored garbage (or a non-integer like '12.5') falls back to the default.
    const n = Number(raw);
    return Number.isInteger(n) && n >= 1 && n <= 65535 ? raw : fallback;
  } catch { return fallback; }
}
function saveVal(key: string, value: string) {
  if (typeof window === 'undefined') return;
  try { window.localStorage.setItem(key, value); } catch { /* ignore */ }
}
function removeVal(key: string) {
  if (typeof window === 'undefined') return;
  try { window.localStorage.removeItem(key); } catch { /* ignore */ }
}
// Strict: '12.5' / '123abc' are not valid ports (Number, not parseInt).
function validPort(v: string): boolean { const n = Number(v); return Number.isInteger(n) && n >= 1 && n <= 65535; }

interface SettingsState {
  opdsPort: string; rssPort: string;
  opdsRunning: boolean; rssRunning: boolean;
  opdsUrl: string | null; rssUrl: string | null;
  opdsBusy: boolean; rssBusy: boolean;
  opdsError: string | null; rssError: string | null;
  syncEnabled: boolean; syncDir: string;
  syncBusy: boolean; syncError: string | null;
  syncStats: { copied: number; skipped: number } | null;
  saveOpdsPort: (v: string) => void; saveRssPort: (v: string) => void;
  startOpds: () => Promise<void>; stopOpds: () => Promise<void>; toggleOpds: () => Promise<void>;
  startRss: () => Promise<void>; stopRss: () => Promise<void>; toggleRss: () => Promise<void>;
  setSyncEnabled: (v: boolean) => void; setSyncDir: (v: string) => void;
  syncNow: () => Promise<void>; syncIfEnabled: () => Promise<void>;
  autoStartAll: () => Promise<void>; reset: () => void;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  opdsPort: loadPort(OPDS_PORT_KEY, '5269'), rssPort: loadPort(RSS_PORT_KEY, '1269'),
  opdsRunning: false, rssRunning: false, opdsUrl: null, rssUrl: null,
  opdsBusy: false, rssBusy: false, opdsError: null, rssError: null,
  syncEnabled: loadVal(SYNC_ENABLED_KEY, '') === '1', syncDir: loadVal(SYNC_DIR_KEY, ''),
  syncBusy: false, syncError: null, syncStats: null,

  // State always accepts the edit (so the input can be cleared mid-typing);
  // only valid values are persisted.
  saveOpdsPort: (v) => { if (validPort(v)) saveVal(OPDS_PORT_KEY, v); set({ opdsPort: v }); },
  saveRssPort: (v) => { if (validPort(v)) saveVal(RSS_PORT_KEY, v); set({ rssPort: v }); },

  startOpds: async () => {
    set({ opdsBusy: true, opdsError: null });
    try { const url = await api.startOpdsServer(parseInt(get().opdsPort, 10)); set({ opdsRunning: true, opdsUrl: url, opdsBusy: false }); }
    catch (e) { set({ opdsError: String(e), opdsBusy: false }); }
  },
  stopOpds: async () => {
    set({ opdsBusy: true, opdsError: null });
    try { await api.stopOpdsServer(); set({ opdsRunning: false, opdsUrl: null, opdsBusy: false }); }
    catch (e) { set({ opdsError: String(e), opdsBusy: false }); }
  },
  toggleOpds: async () => { if (get().opdsRunning) await get().stopOpds(); else await get().startOpds(); },

  startRss: async () => {
    set({ rssBusy: true, rssError: null });
    try { const url = await api.startRssServer(parseInt(get().rssPort, 10)); set({ rssRunning: true, rssUrl: url, rssBusy: false }); }
    catch (e) { set({ rssError: String(e), rssBusy: false }); }
  },
  stopRss: async () => {
    set({ rssBusy: true, rssError: null });
    try { await api.stopRssServer(); set({ rssRunning: false, rssUrl: null, rssBusy: false }); }
    catch (e) { set({ rssError: String(e), rssBusy: false }); }
  },
  toggleRss: async () => { if (get().rssRunning) await get().stopRss(); else await get().startRss(); },

  setSyncEnabled: (v) => { saveVal(SYNC_ENABLED_KEY, v ? '1' : ''); set({ syncEnabled: v }); },
  setSyncDir: (v) => { saveVal(SYNC_DIR_KEY, v); set({ syncDir: v }); },

  syncNow: async () => {
    const { syncEnabled, syncDir, syncBusy } = get();
    if (!syncEnabled || !syncDir || syncBusy) return;
    set({ syncBusy: true, syncError: null });
    try { const stats = await api.syncToDir(syncDir); set({ syncStats: stats, syncBusy: false }); }
    catch (e) { set({ syncError: String(e), syncBusy: false }); }
  },
  syncIfEnabled: async () => { const { syncEnabled, syncDir } = get(); if (syncEnabled && syncDir) await get().syncNow(); },
  autoStartAll: async () => {
    const { opdsRunning, rssRunning } = get();
    const ps: Promise<void>[] = [];
    if (!opdsRunning) ps.push(get().startOpds());
    if (!rssRunning) ps.push(get().startRss());
    await Promise.allSettled(ps);
  },
  reset: () => {
    removeVal(OPDS_PORT_KEY); removeVal(RSS_PORT_KEY); removeVal(SYNC_ENABLED_KEY); removeVal(SYNC_DIR_KEY);
    set({ opdsPort: '5269', rssPort: '1269', opdsRunning: false, rssRunning: false, opdsUrl: null, rssUrl: null, opdsBusy: false, rssBusy: false, opdsError: null, rssError: null, syncEnabled: false, syncDir: '', syncBusy: false, syncError: null, syncStats: null });
  },
}));