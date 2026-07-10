import { defineStore } from 'pinia';
import { ref } from 'vue';

const OPDS_PORT_KEY = 'erolib.opdsPort';
const RSS_PORT_KEY = 'erolib.rssPort';

function loadPort(key: string, fallback: string): string {
  if (typeof window === 'undefined') return fallback;
  const raw = window.localStorage.getItem(key);
  if (!raw) return fallback;
  const n = Number(raw);
  return Number.isInteger(n) && n > 0 && n < 65536 ? raw : fallback;
}

export const useSettingsStore = defineStore('settings', () => {
  const opdsPort = ref(loadPort(OPDS_PORT_KEY, '8080'));
  const rssPort = ref(loadPort(RSS_PORT_KEY, '8081'));

  function saveOpdsPort(value: string) {
    const n = Number(value);
    if (Number.isInteger(n) && n > 0 && n < 65536) {
      opdsPort.value = value;
      window.localStorage.setItem(OPDS_PORT_KEY, value);
    }
  }

  function saveRssPort(value: string) {
    const n = Number(value);
    if (Number.isInteger(n) && n > 0 && n < 65536) {
      rssPort.value = value;
      window.localStorage.setItem(RSS_PORT_KEY, value);
    }
  }

  return {
    opdsPort,
    rssPort,
    saveOpdsPort,
    saveRssPort,
  };
});
