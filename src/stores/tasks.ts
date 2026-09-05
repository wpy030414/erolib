import { create } from 'zustand';
import { api, type TaskItem, type RedownloadAction } from '@/services/api';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

let initPromise: Promise<void> | null = null;
let progressUnlisten: UnlistenFn | null = null;
let toastUnlisten: UnlistenFn | null = null;
let deletedUnlisten: UnlistenFn | null = null;

interface TaskState {
  tasks: TaskItem[]; selectedTaskId: string | null; loading: boolean;
  selectedTask: TaskItem | null;
  init: () => Promise<void>; refresh: () => Promise<void>; selectTask: (id: string | null) => void;
  pauseTask: (id: string) => Promise<void>; resumeTask: (id: string) => Promise<void>;
  cancelTask: (id: string) => Promise<void>; deleteTask: (id: string) => Promise<void>;
  retryTask: (id: string) => Promise<void>; redownloadTask: (id: string) => Promise<RedownloadAction>;
  clearCompleted: () => Promise<number>; retryAll: () => Promise<void>;
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [], selectedTaskId: null, loading: false,
  get selectedTask() { const id = get().selectedTaskId; return id ? get().tasks.find((t) => t.id === id) ?? null : null; },

  init: async () => {
    if (initPromise) return initPromise;
    initPromise = (async () => {
      set({ loading: true });
      await get().refresh();

      progressUnlisten = await listen<TaskItem>('task://progress', (event) => {
        const incoming = event.payload;
        set((s) => { const idx = s.tasks.findIndex((t) => t.id === incoming.id); return idx >= 0 ? { tasks: s.tasks.map((t, i) => i === idx ? incoming : t) } : { tasks: [incoming, ...s.tasks] }; });
      });

      toastUnlisten = await listen<{ id: string; status: string; title: string; book_id?: string }>('task://toast', async (event) => {
        const { status, title } = event.payload;
        const { useToastStore } = await import('./toast');
        const { t } = await import('@/i18n/index');
        if (status === 'completed') {
          useToastStore.getState().addToast('success', t('tasks.toast.completed', { title }));
          const { useLibraryStore } = await import('./library');
          await useLibraryStore.getState().refresh();
          const { useSettingsStore } = await import('./settings');
          await useSettingsStore.getState().syncIfEnabled();
        } else if (status === 'failed') useToastStore.getState().addToast('error', t('tasks.toast.failed', { title }));
        else if (status === 'cancelled') useToastStore.getState().addToast('info', t('tasks.toast.cancelled', { title }));
      });

      deletedUnlisten = await listen<{ book_id: string }>('book://deleted', (event) => {
        set((s) => ({ tasks: s.tasks.map((t) => t.book_id === event.payload.book_id ? { ...t, book_id: undefined } : t) }));
      });

      set({ loading: false });
    })();
    return initPromise;
  },

  refresh: async () => { try { const tasks = await api.tasksList(); set({ tasks }); } catch { /* ignore */ } },
  selectTask: (id) => set({ selectedTaskId: id }),

  pauseTask: async (id) => { await api.taskPause(id); await get().refresh(); },
  resumeTask: async (id) => { await api.taskResume(id); await get().refresh(); },
  cancelTask: async (id) => { await api.taskCancel(id); await get().refresh(); },
  deleteTask: async (id) => {
    await api.taskDelete(id);
    if (get().selectedTaskId === id) set({ selectedTaskId: null });
    await get().refresh();
    const { useToastStore } = await import('./toast');
    const { t } = await import('@/i18n/index');
    useToastStore.getState().addToast('success', t('tasks.toast.deleted'));
  },
  retryTask: async (id) => { await api.taskRetry(id); await get().refresh(); },
  redownloadTask: async (id) => { const r = await api.taskRedownload(id); await get().refresh(); return r; },
  clearCompleted: async () => {
    try {
      await api.tasksClearCompleted();
      const count = get().tasks.filter((t) => t.status === 'completed').length;
      if (get().selectedTaskId && get().tasks.some((t) => t.id === get().selectedTaskId && t.status === 'completed')) set({ selectedTaskId: null });
      await get().refresh();
      if (count > 0) { const { useToastStore } = await import('./toast'); const { t } = await import('@/i18n/index'); useToastStore.getState().addToast('success', t('tasks.toast.cleared', { count })); }
      return count;
    } catch { return 0; }
  },
  retryAll: async () => { await api.tasksRetryAll(); await get().refresh(); },
}));