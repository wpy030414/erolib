import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/services/api';

const OPDS_PORT_KEY = 'erolib.opdsPort';
const RSS_PORT_KEY = 'erolib.rssPort';

const LOCAL_SYNC_ENABLED_KEY = 'erolib.localSyncEnabled';
const LOCAL_SYNC_DIR_KEY = 'erolib.localSyncDir';

// Default ports: 5269 for OPDS, 1269 for RSS
const DEFAULT_OPDS_PORT = '5269';
const DEFAULT_RSS_PORT = '1269';

function loadPort(key: string, fallback: string): string {
  if (typeof window === 'undefined') return fallback;
  const raw = window.localStorage.getItem(key);
  if (!raw) return fallback;
  const n = Number(raw);
  return Number.isInteger(n) && n > 0 && n < 65536 ? raw : fallback;
}

function savePort(key: string, value: string) {
  if (typeof window === 'undefined') return;
  const n = Number(value);
  if (Number.isInteger(n) && n > 0 && n < 65536) {
    window.localStorage.setItem(key, value);
  }
}

export const useSettingsStore = defineStore('settings', () => {
  const opdsPort = ref(loadPort(OPDS_PORT_KEY, DEFAULT_OPDS_PORT));
  const rssPort = ref(loadPort(RSS_PORT_KEY, DEFAULT_RSS_PORT));
  const opdsRunning = ref(false);
  const rssRunning = ref(false);
  const opdsUrl = ref<string | null>(null);
  const rssUrl = ref<string | null>(null);
  const opdsBusy = ref(false);
  const rssBusy = ref(false);
  const opdsError = ref<string | null>(null);
  const rssError = ref<string | null>(null);

  // One-way local sync (library → a chosen directory). Persisted in localStorage
  // like the server ports; the actual mirror runs in the backend.
  const syncEnabled = ref(
    typeof window !== 'undefined' && window.localStorage.getItem(LOCAL_SYNC_ENABLED_KEY) === '1',
  );
  const syncDir = ref(
    (typeof window !== 'undefined' && window.localStorage.getItem(LOCAL_SYNC_DIR_KEY)) || '',
  );
  const syncBusy = ref(false);
  const syncError = ref<string | null>(null);
  const syncStats = ref<{ copied: number; skipped: number } | null>(null);

  function saveOpdsPort(value: string) {
    savePort(OPDS_PORT_KEY, value);
    opdsPort.value = value;
  }

  function saveRssPort(value: string) {
    savePort(RSS_PORT_KEY, value);
    rssPort.value = value;
  }

  async function startOpds() {
    opdsBusy.value = true;
    opdsError.value = null;
    try {
      opdsUrl.value = await api.startOpdsServer(Number(opdsPort.value));
      opdsRunning.value = true;
    } catch (e) {
      opdsError.value = String(e);
    } finally {
      opdsBusy.value = false;
    }
  }

  async function stopOpds() {
    opdsBusy.value = true;
    opdsError.value = null;
    try {
      await api.stopOpdsServer();
      opdsUrl.value = null;
      opdsRunning.value = false;
    } catch (e) {
      opdsError.value = String(e);
    } finally {
      opdsBusy.value = false;
    }
  }

  async function toggleOpds() {
    if (opdsRunning.value) await stopOpds();
    else await startOpds();
  }

  async function startRss() {
    rssBusy.value = true;
    rssError.value = null;
    try {
      rssUrl.value = await api.startRssServer(Number(rssPort.value));
      rssRunning.value = true;
    } catch (e) {
      rssError.value = String(e);
    } finally {
      rssBusy.value = false;
    }
  }

  async function stopRss() {
    rssBusy.value = true;
    rssError.value = null;
    try {
      await api.stopRssServer();
      rssUrl.value = null;
      rssRunning.value = false;
    } catch (e) {
      rssError.value = String(e);
    } finally {
      rssBusy.value = false;
    }
  }

  async function toggleRss() {
    if (rssRunning.value) await stopRss();
    else await startRss();
  }

  function setSyncEnabled(v: boolean) {
    syncEnabled.value = v;
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(LOCAL_SYNC_ENABLED_KEY, v ? '1' : '0');
    }
  }

  function setSyncDir(v: string) {
    syncDir.value = v;
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(LOCAL_SYNC_DIR_KEY, v);
    }
  }

  /** Run the mirror now (copy new books, delete removed). No-op unless enabled
   *  with a target directory and not already busy. */
  async function syncNow() {
    if (!syncEnabled.value || !syncDir.value || syncBusy.value) return;
    syncBusy.value = true;
    syncError.value = null;
    try {
      syncStats.value = await api.syncToDir(syncDir.value);
    } catch (e) {
      syncError.value = String(e);
    } finally {
      syncBusy.value = false;
    }
  }

  /** Called by library/tasks stores on book changes — syncs only if the user
   *  has enabled it, so it's safe to wire into every mutation. */
  async function syncIfEnabled() {
    if (syncEnabled.value && syncDir.value) await syncNow();
  }

  /** App.vue calls this on mount so the sharing servers are running the moment
   *  the app opens, using the saved ports (single source of truth). */
  async function autoStartAll() {
    await Promise.all([startOpds(), startRss()]);
  }

  function reset() {
    opdsPort.value = DEFAULT_OPDS_PORT;
    rssPort.value = DEFAULT_RSS_PORT;
    syncEnabled.value = false;
    syncDir.value = '';
    window.localStorage.removeItem(OPDS_PORT_KEY);
    window.localStorage.removeItem(RSS_PORT_KEY);
    window.localStorage.removeItem(LOCAL_SYNC_ENABLED_KEY);
    window.localStorage.removeItem(LOCAL_SYNC_DIR_KEY);
  }

  return {
    opdsPort,
    rssPort,
    opdsRunning,
    rssRunning,
    opdsUrl,
    rssUrl,
    opdsBusy,
    rssBusy,
    opdsError,
    rssError,
    syncEnabled,
    syncDir,
    syncBusy,
    syncError,
    syncStats,
    saveOpdsPort,
    saveRssPort,
    startOpds,
    stopOpds,
    toggleOpds,
    startRss,
    stopRss,
    toggleRss,
    setSyncEnabled,
    setSyncDir,
    syncNow,
    syncIfEnabled,
    autoStartAll,
    reset,
  };
});
