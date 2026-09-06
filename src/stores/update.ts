import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api, type UpdateInfo, type UpdateProgress } from '@/services/api';

let progressUnlisten: UnlistenFn | null = null;
let listenerInit = false;

async function initProgressListener() {
  if (listenerInit) return;
  listenerInit = true;
  progressUnlisten = await listen<UpdateProgress>('update://progress', (event) => {
    useUpdateStore.setState({ progress: event.payload });
  });
}

interface UpdateState {
  info: UpdateInfo | null; checking: boolean; downloading: boolean;
  downloadPath: string | null; progress: UpdateProgress; error: string | null;
  check: () => Promise<void>; download: () => Promise<void>;
  install: () => void; quitAndInstall: () => void; clearDownload: () => void;
}

export const useUpdateStore = create<UpdateState>((set, get) => ({
  info: null, checking: false, downloading: false, downloadPath: null,
  progress: { percent: 0, speed: 0, completed: 0, total: 0 }, error: null,

  check: async () => {
    set({ checking: true, error: null });
    try { const info = await api.checkUpdate(); set({ info, checking: false }); }
    // A failed check invalidates the previous result.
    catch (e) { set({ error: String(e), checking: false, info: null }); }
  },
  download: async () => {
    const { info } = get(); if (!info?.asset) return;
    await initProgressListener();
    set({ downloading: true, error: null });
    try {
      const path = await api.downloadUpdate(info.asset.url, info.asset.name);
      set({ downloadPath: path, downloading: false });
      const { useToastStore } = await import('./toast');
      const { t } = await import('@/i18n/index');
      useToastStore.getState().addToast('success', t('settings.update.downloadComplete'));
    } catch (e) {
      set({ error: String(e), downloading: false });
      const { useToastStore } = await import('./toast');
      const { t } = await import('@/i18n/index');
      useToastStore.getState().addToast('error', t('settings.update.downloadFailed', { error: String(e) }));
    }
  },
  install: () => {
    const p = get().downloadPath;
    if (!p) return;
    api.installUpdate(p).catch(async (e) => {
      const { useToastStore } = await import('./toast');
      useToastStore.getState().addToast('error', String(e));
    });
  },
  quitAndInstall: () => {
    const p = get().downloadPath;
    if (!p) return;
    api.quitAndInstall(p).catch(async (e) => {
      const { useToastStore } = await import('./toast');
      useToastStore.getState().addToast('error', String(e));
    });
  },
  clearDownload: () => set({ downloadPath: null, progress: { percent: 0, speed: 0, completed: 0, total: 0 } }),
}));