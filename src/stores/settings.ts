import { defineStore } from 'pinia';
import { ref } from 'vue';

const OPDS_PORT_KEY = 'erolib.opdsPort';
const RSS_PORT_KEY = 'erolib.rssPort';

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

  function saveOpdsPort(value: string) {
    savePort(OPDS_PORT_KEY, value);
    opdsPort.value = value;
  }

  function saveRssPort(value: string) {
    savePort(RSS_PORT_KEY, value);
    rssPort.value = value;
  }

  function reset() {
    opdsPort.value = DEFAULT_OPDS_PORT;
    rssPort.value = DEFAULT_RSS_PORT;
    window.localStorage.removeItem(OPDS_PORT_KEY);
    window.localStorage.removeItem(RSS_PORT_KEY);
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
    saveOpdsPort,
    saveRssPort,
    reset,
  };
});
