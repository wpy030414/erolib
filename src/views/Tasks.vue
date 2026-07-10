<template>
  <div class="tasks-view pa-6">
    <h2 class="text-h5 mb-6">{{ t('tasks.title') }}</h2>

    <div v-if="tasks.length === 0" class="empty-state">
      <p class="text-body-1 text-medium-emphasis">{{ t('tasks.empty') }}</p>
    </div>

    <div v-else class="task-list">
      <div
        v-for="item in tasks"
        :key="item.id"
        class="md3-card md3-card--outlined task-card"
      >
        <div class="task-header">
          <span class="task-title">{{ item.title }}</span>
          <span class="task-status" :class="'status--' + item.status">
            {{ t('tasks.status.' + item.status) }}
          </span>
        </div>

        <div v-if="item.detail" class="task-detail text-body-2 text-medium-emphasis">
          {{ item.detail }}
        </div>

        <div class="task-progress">
          <div class="progress-bar-bg">
            <div
              class="progress-bar-fill"
              :style="{ width: progressPercent(item) + '%' }"
            />
          </div>
          <span class="progress-text text-body-3">
            {{ item.progress_current }} / {{ item.progress_total }}
          </span>
        </div>

        <div class="task-actions">
          <md-filled-tonal-button
            v-if="item.status === 'running'"
            @click="taskStore.pauseTask(item.id)"
          >
            <MdiIcon slot="icon" :path="mdiPause" :size="18" />
            {{ t('tasks.actions.pause') }}
          </md-filled-tonal-button>

          <md-filled-tonal-button
            v-if="item.status === 'paused'"
            @click="taskStore.resumeTask(item.id)"
          >
            <MdiIcon slot="icon" :path="mdiPlay" :size="18" />
            {{ t('tasks.actions.resume') }}
          </md-filled-tonal-button>

          <md-filled-tonal-button
            v-if="item.status === 'running' || item.status === 'paused'"
            @click="taskStore.cancelTask(item.id)"
          >
            <MdiIcon slot="icon" :path="mdiClose" :size="18" />
            {{ t('tasks.actions.cancel') }}
          </md-filled-tonal-button>

          <md-filled-tonal-button
            v-if="item.status === 'failed'"
            @click="taskStore.retryTask(item.id)"
          >
            <MdiIcon slot="icon" :path="mdiRefresh" :size="18" />
            {{ t('tasks.actions.retry') }}
          </md-filled-tonal-button>

          <md-outlined-button
            v-if="item.status === 'completed' || item.status === 'failed' || item.status === 'cancelled'"
            @click="taskStore.deleteTask(item.id)"
          >
            <MdiIcon slot="icon" :path="mdiDelete" :size="18" />
            {{ t('tasks.actions.delete') }}
          </md-outlined-button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue';
import {
  mdiPause,
  mdiPlay,
  mdiClose,
  mdiDelete,
  mdiRefresh,
} from '@mdi/js';
import { useI18n } from '@/i18n';
import { useTaskStore } from '@/stores/tasks';
import MdiIcon from '@/components/MdiIcon.vue';

const { t } = useI18n();
const taskStore = useTaskStore();
const { tasks } = taskStore;

function progressPercent(item: { progress_current: number; progress_total: number }): number {
  if (item.progress_total <= 0) return 0;
  return Math.min(100, Math.round((item.progress_current / item.progress_total) * 100));
}

onMounted(() => {
  taskStore.init();
});
</script>

<style scoped>
.tasks-view {
  max-width: 800px;
}

.task-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.task-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  border-radius: var(--md-sys-shape-corner-medium);
  background: var(--md-sys-color-surface);
  border: 1px solid var(--md-sys-color-outline-variant);
}

.task-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.task-title {
  font: 500 var(--md-sys-typescale-title-medium-size) / var(--md-sys-typescale-title-medium-line-height) var(--md-sys-typescale-font);
  color: var(--md-sys-color-on-surface);
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-status {
  font: 500 var(--md-sys-typescale-label-medium-size) / var(--md-sys-typescale-label-medium-line-height) var(--md-sys-typescale-font);
  padding: 2px 10px;
  border-radius: var(--md-sys-shape-corner-small);
  flex-shrink: 0;
}

.status--running {
  background: var(--md-sys-color-tertiary-container);
  color: var(--md-sys-color-on-tertiary-container);
}

.status--pending {
  background: var(--md-sys-color-secondary-container);
  color: var(--md-sys-color-on-secondary-container);
}

.status--paused {
  background: var(--md-sys-color-surface-variant);
  color: var(--md-sys-color-on-surface-variant);
}

.status--completed {
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
}

.status--failed {
  background: #fce4ec;
  color: #c62828;
}

.status--cancelled {
  background: var(--md-sys-color-surface-variant);
  color: var(--md-sys-color-on-surface-variant);
}

.task-progress {
  display: flex;
  align-items: center;
  gap: 12px;
}

.progress-bar-bg {
  flex: 1;
  height: 6px;
  background: var(--md-sys-color-surface-variant);
  border-radius: 3px;
  overflow: hidden;
}

.progress-bar-fill {
  height: 100%;
  background: var(--md-sys-color-primary);
  border-radius: 3px;
  transition: width 0.3s ease;
}

.progress-text {
  flex-shrink: 0;
  color: var(--md-sys-color-on-surface-variant);
}

.task-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
}
</style>
