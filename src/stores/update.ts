import { ref } from 'vue';
import { defineStore } from 'pinia';
import { listen } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import type { UpdateInfo, UpdateProgress } from '@/services/api';
import { useToastStore } from './toast';
import { useI18n } from '@/i18n';

let listenerInit = false;

export const useUpdateStore = defineStore('update', () => {
  const info = ref<UpdateInfo | null>(null);
  const checking = ref(false);
  const downloading = ref(false);
  const downloadPath = ref<string | null>(null);
  const progress = ref<UpdateProgress>({ percent: 0, speed: 0, completed: 0, total: 0 });
  const error = ref<string | null>(null);
  const toastStore = useToastStore();
  const { t } = useI18n();

  function initProgressListener() {
    if (listenerInit) return;
    listenerInit = true;
    listen<UpdateProgress>('update://progress', (event) => {
      progress.value = event.payload;
    });
  }

  async function check() {
    checking.value = true;
    error.value = null;
    try {
      info.value = await api.checkUpdate();
    } catch (e) {
      error.value = typeof e === 'string' ? e : String(e);
      info.value = null;
    } finally {
      checking.value = false;
    }
  }

  async function download() {
    if (!info.value?.asset) return;
    initProgressListener();

    downloading.value = true;
    downloadPath.value = null;
    progress.value = { percent: 0, speed: 0, completed: 0, total: 0 };
    error.value = null;

    try {
      downloadPath.value = await api.downloadUpdate(
        info.value.asset.url,
        info.value.asset.name,
      );
      toastStore.addToast('success', t('settings.update.downloadComplete'));
    } catch (e) {
      error.value = typeof e === 'string' ? e : String(e);
      toastStore.addToast('error', t('settings.update.downloadFailed', { error: error.value! }));
    } finally {
      downloading.value = false;
    }
  }

  function install() {
    if (!downloadPath.value) return;
    api.installUpdate(downloadPath.value).catch((e) => {
      toastStore.addToast('error', String(e));
    });
  }

  function quitAndInstall() {
    if (!downloadPath.value) return;
    api.quitAndInstall(downloadPath.value).catch((e) => {
      toastStore.addToast('error', String(e));
    });
  }

  function clearDownload() {
    downloadPath.value = null;
    progress.value = { percent: 0, speed: 0, completed: 0, total: 0 };
  }

  return {
    info,
    checking,
    downloading,
    downloadPath,
    progress,
    error,
    check,
    download,
    install,
    quitAndInstall,
    clearDownload,
  };
})
