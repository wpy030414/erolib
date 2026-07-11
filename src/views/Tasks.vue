<template>
  <div class="tasks-view pa-6">
    <div class="tasks-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 tasks-header__title">{{ t('tasks.title') }}</h2>
    </div>

    <div v-if="tasks.length === 0" class="empty-state">
      <p class="text-body-1 text-medium-emphasis">{{ t('tasks.empty') }}</p>
    </div>

    <div v-else class="task-list">
      <div
        v-for="item in tasks"
        :key="item.id"
        class="md3-card md3-card--outlined task-card"
        :class="{ 'task-card--selected': selectedTaskId === item.id }"
        @click="selectTask(item.id)"
      >
        <div class="task-header">
          <span class="task-title">{{ item.title }}</span>
          <span class="task-status" :class="'status--' + item.status">
            {{ t('tasks.status.' + item.status) }}
          </span>
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

        <!-- Inline logs: expand below the progress bar when this card is selected. -->
        <div class="task-logs-wrap" :class="{ 'task-logs-wrap--open': selectedTaskId === item.id }">
          <div class="task-logs-inner">
            <div v-if="item.logs.length" class="logs-list">
              <div
                v-for="(line, i) in item.logs"
                :key="i"
                class="log-line text-body-2"
              >
                {{ line }}
              </div>
            </div>
            <p v-else class="text-body-2 text-medium-emphasis logs-empty">
              {{ t('tasks.detail.noLogs') }}
            </p>
          </div>
        </div>

        <div class="task-footer">
          <div class="task-actions">
            <md-filled-button
              v-if="item.status === 'completed' && item.book_id"
              @click.stop="readBook(item.book_id)"
            >
              <MdiIcon slot="icon" :path="mdiBookOpen" :size="18" />
              {{ t('tasks.actions.read') }}
            </md-filled-button>

            <md-filled-tonal-button
              v-if="item.status === 'running'"
              @click.stop="taskStore.pauseTask(item.id)"
            >
              <MdiIcon slot="icon" :path="mdiPause" :size="18" />
              {{ t('tasks.actions.pause') }}
            </md-filled-tonal-button>

            <md-filled-tonal-button
              v-if="item.status === 'paused'"
              @click.stop="taskStore.resumeTask(item.id)"
            >
              <MdiIcon slot="icon" :path="mdiPlay" :size="18" />
              {{ t('tasks.actions.resume') }}
            </md-filled-tonal-button>

            <md-filled-tonal-button
              v-if="item.status === 'running' || item.status === 'paused'"
              @click.stop="taskStore.cancelTask(item.id)"
            >
              <MdiIcon slot="icon" :path="mdiClose" :size="18" />
              {{ t('tasks.actions.cancel') }}
            </md-filled-tonal-button>

            <md-filled-tonal-button
              v-if="item.status === 'failed'"
              @click.stop="taskStore.retryTask(item.id)"
            >
              <MdiIcon slot="icon" :path="mdiRefresh" :size="18" />
              {{ t('tasks.actions.retry') }}
            </md-filled-tonal-button>

            <md-outlined-button
              v-if="item.status === 'completed' || item.status === 'failed' || item.status === 'cancelled'"
              @click.stop="taskStore.deleteTask(item.id)"
            >
              <MdiIcon slot="icon" :path="mdiDelete" :size="18" />
              {{ t('tasks.actions.remove') }}
            </md-outlined-button>
          </div>

          <span v-if="item.status === 'running'" class="task-speed">
            {{ formatSpeed(item.speed) }}
          </span>
          <span v-else-if="item.status === 'completed'" class="task-speed">
            {{ t('tasks.summary', { size: formatBytes(item.total_bytes), time: formatDuration(item.elapsed_ms) }) }}
          </span>
        </div>
      </div>
    </div>

    <button
      v-if="hasCompleted"
      class="fab-clear"
      :aria-label="t('tasks.actions.clearCompleted')"
      :disabled="clearing"
      @click="onClearCompleted"
    >
      <MdiIcon :path="mdiBroom" :size="24" />
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import {
  mdiPause,
  mdiPlay,
  mdiClose,
  mdiDelete,
  mdiRefresh,
  mdiBookOpen,
  mdiBroom,
} from '@mdi/js';
import { useI18n } from '@/i18n';
import { useTaskStore } from '@/stores/tasks';
import { useToastStore } from '@/stores/toast';
import MdiIcon from '@/components/MdiIcon.vue';

const { t } = useI18n();
const router = useRouter();
const toastStore = useToastStore();
const taskStore = useTaskStore();
const { tasks, selectedTaskId } = taskStore;

const clearing = ref(false);

const TERMINAL = ['completed', 'failed', 'cancelled'];
const hasCompleted = computed(() => tasks.value.some((tk) => TERMINAL.includes(tk.status)));

function progressPercent(item: { progress_current: number; progress_total: number }): number {
  if (item.progress_total <= 0) return 0;
  return Math.min(100, Math.round((item.progress_current / item.progress_total) * 100));
}

function formatSpeed(bps: number): string {
  // Persistently shown while running — never blank, so the readout doesn't
  // flicker between pages. 0 B/s is shown during inter-page gaps.
  if (bps <= 0) return t('tasks.speed.kbps', { speed: '0.0' });
  if (bps < 1024 * 1024) {
    return t('tasks.speed.kbps', { speed: (bps / 1024).toFixed(1) });
  }
  return t('tasks.speed.mbps', { speed: (bps / 1024 / 1024).toFixed(2) });
}

function formatBytes(b: number): string {
  if (b <= 0) return t('tasks.size.mb', { size: '0' });
  const mb = b / (1024 * 1024);
  if (mb >= 1024) return t('tasks.size.gb', { size: (mb / 1024).toFixed(2) });
  if (mb >= 1) return t('tasks.size.mb', { size: mb.toFixed(1) });
  return t('tasks.size.kb', { size: (b / 1024).toFixed(1) });
}

function formatDuration(ms: number): string {
  // Units are dropped from the top when they're zero: "3分20秒", "45秒",
  // never "0时3分20秒".
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  if (h > 0) return t('tasks.duration.hms', { h, m, s });
  if (m > 0) return t('tasks.duration.ms', { m, s });
  return t('tasks.duration.s', { s });
}

function selectTask(id: string) {
  // Toggle: clicking the open card again collapses it.
  taskStore.selectTask(selectedTaskId.value === id ? null : id);
}

function readBook(bookId: string) {
  router.push(`/reader/${bookId}`);
}

async function onClearCompleted() {
  clearing.value = true;
  try {
    const before = tasks.value.filter((tk) => TERMINAL.includes(tk.status)).length;
    await taskStore.clearCompleted();
    toastStore.addToast('info', t('tasks.toast.cleared', { count: before }));
  } catch (e) {
    toastStore.addToast('error', t('common.error', { message: String(e) }));
  } finally {
    clearing.value = false;
  }
}

onMounted(() => {
  taskStore.init();
});
</script>

<style scoped>
/* Title aligns with the other views — clear the h2 default margin-block that
   was making this header row taller than Library/Pixiv/EHentai. */
.tasks-header__title {
  margin: 0;
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
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    border-color 0.15s ease;
}

.task-card:hover {
  background: var(--md-sys-color-surface-container);
}

.task-card--selected {
  border-color: var(--md-sys-color-primary);
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

.task-detail-line {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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

/* Inline logs: collapsed by default, smoothly expand when the card is selected.
   max-height transition gives the "quickly grow taller" effect. */
.task-logs-wrap {
  max-height: 0;
  overflow: hidden;
  opacity: 0;
  transition:
    max-height 0.2s ease,
    opacity 0.2s ease,
    margin 0.2s ease;
}

.task-logs-wrap--open {
  /* Generous ceiling; the inner list itself scrolls beyond this. */
  max-height: 240px;
  opacity: 1;
}

.task-logs-inner {
  display: flex;
  flex-direction: column;
}

.logs-list {
  max-height: 220px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px;
  border-radius: var(--md-sys-shape-corner-small);
  background: var(--md-sys-color-surface-variant);
}

.logs-empty {
  padding: 8px 10px;
}

.log-line {
  color: var(--md-sys-color-on-surface-variant);
  font-family: var(--md-sys-typescale-font);
  font-variant-numeric: tabular-nums;
  word-break: break-word;
  white-space: pre-wrap;
}

.task-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.task-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.task-speed {
  font: 500 var(--md-sys-typescale-label-medium-size) / var(--md-sys-typescale-label-medium-line-height) var(--md-sys-typescale-font);
  font-variant-numeric: tabular-nums;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
  min-width: 78px;
  text-align: right;
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
}

.tasks-header {
  flex-wrap: wrap;
  /* Match the row height the other views get from their header button (40px is
     the MD3 button container height), so this title aligns vertically with
     Library/Pixiv/EHentai even though it has no action button. */
  min-height: 40px;
}

.spacer {
  flex: 1 1 auto;
}

/* Floating "clear completed" button (bottom-right). */
.fab-clear {
  position: fixed;
  right: 24px;
  bottom: 24px;
  z-index: 50;
  width: 56px;
  height: 56px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
  background: var(--md-sys-color-primary);
  color: var(--md-sys-color-on-primary);
  box-shadow: var(--md-sys-elevation-level3);
  cursor: pointer;
  transition:
    box-shadow 0.15s ease,
    transform 0.15s ease;
}

.fab-clear:hover:not(:disabled) {
  box-shadow: var(--md-sys-elevation-level4);
  transform: scale(1.05);
}

.fab-clear:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
