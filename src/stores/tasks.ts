import { ref, computed } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import type { TaskItem } from '@/services/api';
import { useToastStore } from './toast';
import { useLibraryStore } from './library';
import { useSettingsStore } from './settings';
import { useI18n } from '@/i18n';

export type { TaskItem };

const tasks = ref<TaskItem[]>([]);
const selectedTaskId = ref<string | null>(null);
let initialized = false;

export function useTaskStore() {
  const toastStore = useToastStore();
  const { t } = useI18n();

  async function init() {
    if (initialized) return;
    initialized = true;
    await refresh();

    await listen<TaskItem>('task://progress', (event) => {
      const idx = tasks.value.findIndex((t) => t.id === event.payload.id);
      if (idx !== -1) {
        tasks.value[idx] = event.payload;
      } else {
        tasks.value.unshift(event.payload);
      }
    });

    await listen<{ kind: string; title: string }>('task://toast', (event) => {
      const { kind, title } = event.payload;
      if (kind === 'completed') {
        toastStore.addToast('success', t('tasks.toast.completed', { title }));
        // A finished download produced new library data — refresh the shelf.
        useLibraryStore().refresh().catch(() => {});
        // And push it to the local sync folder if enabled.
        useSettingsStore().syncIfEnabled();
      } else if (kind === 'failed') {
        toastStore.addToast('error', t('tasks.toast.failed', { title }));
      } else if (kind === 'cancelled') {
        toastStore.addToast('info', t('tasks.toast.cancelled', { title }));
      }
    });

    // A book deleted from the library detaches from its task: clear the
    // book_id so the "Read" button (v-if item.book_id) disappears.
    await listen<{ bookId: string }>('book://deleted', (event) => {
      const idx = tasks.value.findIndex((t) => t.book_id === event.payload.bookId);
      if (idx !== -1) {
        tasks.value[idx] = { ...tasks.value[idx], book_id: null };
      }
    });
  }

  async function refresh() {
    try {
      tasks.value = await api.tasksList();
    } catch (e) {
      console.error('Failed to refresh tasks', e);
    }
  }

  async function pauseTask(id: string) {
    await api.taskPause(id);
    await refresh();
  }

  async function resumeTask(id: string) {
    await api.taskResume(id);
    await refresh();
  }

  async function cancelTask(id: string) {
    await api.taskCancel(id);
    await refresh();
  }

  function selectTask(id: string | null) {
    selectedTaskId.value = id;
  }

  async function deleteTask(id: string) {
    const task = tasks.value.find((tk) => tk.id === id);
    await api.taskDelete(id);
    if (selectedTaskId.value === id) {
      selectedTaskId.value = null;
    }
    await refresh();
    toastStore.addToast('info', t('tasks.toast.removed', { title: task?.title ?? '' }));
  }

  async function retryTask(id: string) {
    await api.taskRetry(id);
    await refresh();
  }

  async function clearCompleted() {
    await api.tasksClearCompleted();
    // Drop any selected task that was just cleared.
    if (selectedTaskId.value) {
      const stillExists = tasks.value.some((tk) => tk.id === selectedTaskId.value);
      if (!stillExists) selectedTaskId.value = null;
    }
    await refresh();
  }

  const selectedTask = computed(() =>
    tasks.value.find((t) => t.id === selectedTaskId.value) ?? null,
  );

  return {
    tasks,
    selectedTaskId,
    selectedTask,
    init,
    refresh,
    selectTask,
    pauseTask,
    resumeTask,
    cancelTask,
    deleteTask,
    retryTask,
    clearCompleted,
  };
}
