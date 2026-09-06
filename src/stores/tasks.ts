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
  clearCompleted: () => Promise<void>; retryAll: () => Promise<void>;
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

      // Backend TaskToast = { kind, title } (kind: completed|failed|cancelled).
      toastUnlisten = await listen<{ kind: string; title: string }>('task://toast', async (event) => {
        const { kind, title } = event.payload;
        const { useToastStore } = await import('./toast');
        const { t } = await import('@/i18n/index');
        if (kind === 'completed') {
          useToastStore.getState().addToast('success', t('tasks.toast.completed', { title }));
          const { useLibraryStore } = await import('./library');
          await useLibraryStore.getState().refresh().catch(() => {});
          // Local one-way sync picks up the new book (no-op unless enabled).
          const { useSettingsStore } = await import('./settings');
          await useSettingsStore.getState().syncIfEnabled();
        } else if (kind === 'failed') useToastStore.getState().addToast('error', t('tasks.toast.failed', { title }));
        else if (kind === 'cancelled') useToastStore.getState().addToast('info', t('tasks.toast.cancelled', { title }));
      });

      // Backend emits { bookId } (camelCase).
      deletedUnlisten = await listen<{ bookId: string }>('book://deleted', (event) => {
        set((s) => ({ tasks: s.tasks.map((t) => t.book_id === event.payload.bookId ? { ...t, book_id: null } : t) }));
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
    const title = get().tasks.find((t) => t.id === id)?.title ?? '';
    await api.taskDelete(id);
    if (get().selectedTaskId === id) set({ selectedTaskId: null });
    await get().refresh();
    const { useToastStore } = await import('./toast');
    const { t } = await import('@/i18n/index');
    useToastStore.getState().addToast('info', t('tasks.toast.removed', { title }));
  },
  retryTask: async (id) => { await api.taskRetry(id); await get().refresh(); },
  redownloadTask: async (id) => { const r = await api.taskRedownload(id); await get().refresh(); return r; },
  clearCompleted: async () => {
    await api.tasksClearCompleted();
    // 语义对齐 Vue：选中折叠发生在 API 成功之后——失败路径保留选中，
    // 成功后 completed 任务已消失、选中自然作废。
    set({ selectedTaskId: null });
    await get().refresh();
  },
  retryAll: async () => { await api.tasksRetryAll(); await get().refresh(); },
}));