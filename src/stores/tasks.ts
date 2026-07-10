export interface TaskItem {
  id: string;
  source: string;
  status: string;
  title: string;
  detail: string;
  progress_current: number;
  progress_total: number;
  retry_count: number;
  max_retries: number;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

import { ref } from 'vue';
import { listen } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { useToastStore } from './toast';
import { useLibraryStore } from './library';
import { useI18n } from '@/i18n';

const tasks = ref<TaskItem[]>([]);
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
      } else if (kind === 'failed') {
        toastStore.addToast('error', t('tasks.toast.failed', { title }));
      } else if (kind === 'cancelled') {
        toastStore.addToast('info', t('tasks.toast.cancelled', { title }));
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

  async function deleteTask(id: string) {
    await api.taskDelete(id);
    await refresh();
  }

  async function retryTask(id: string) {
    await api.taskRetry(id);
    await refresh();
  }

  return {
    tasks,
    init,
    refresh,
    pauseTask,
    resumeTask,
    cancelTask,
    deleteTask,
    retryTask,
  };
}
