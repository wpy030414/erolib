<template>
  <dialog ref="dialogEl" class="export-dialog" @click="onBackdrop" @cancel="onEscape">
    <div v-if="book" class="export-dialog__panel">
      <div class="export-dialog__header">
        <span class="export-dialog__title">
          {{ exporting ? t('lib.save.exporting') : t('lib.save.format') }}
        </span>
        <button v-if="!exporting" class="icon-btn" :aria-label="t('common.dismiss')" @click="close">
          <MdiIcon :path="mdiClose" :size="20" />
        </button>
      </div>

      <p class="export-dialog__subtitle">{{ book.title }}</p>

      <!-- Format selection -->
      <template v-if="!exporting">
        <div class="format-options">
          <button
            v-for="opt in formats"
            :key="opt.value"
            class="format-option"
            :class="{ 'format-option--selected': selected === opt.value }"
            @click="selected = opt.value"
          >
            <MdiIcon :path="opt.icon" :size="22" />
            <span class="format-option__label">{{ opt.label }}</span>
            <span class="format-option__desc">{{ opt.desc }}</span>
          </button>
        </div>

        <div class="export-dialog__actions">
          <md-outlined-button @click="close">
            {{ t('common.cancel') }}
          </md-outlined-button>
          <md-filled-button :disabled="busy" @click="confirm">
            {{ t('lib.save') }}
          </md-filled-button>
        </div>
      </template>

      <!-- Progress: replaces the format options once the destination is
           picked; the dialog closes itself when the export finishes. -->
      <template v-else>
        <div class="export-progress">
          <md-linear-progress :value="progressValue" />
          <p class="export-progress__label">
            {{ t('lib.save.exportingProgress', { done: progress.done, total: progress.total }) }}
          </p>
        </div>
      </template>
    </div>
  </dialog>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue';
import { save as dialogSave } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import '@material/web/button/outlined-button.js';
import '@material/web/progress/linear-progress.js';
import {
  mdiClose,
  mdiArchive,
  mdiBookOpen,
  mdiFilePdfBox,
} from '@mdi/js';
import { useI18n } from '@/i18n';
import { api, type ExportProgress } from '@/services/api';
import { useToastStore } from '@/stores/toast';
import MdiIcon from '@/components/MdiIcon.vue';
import type { Book } from '@/types';

const { t } = useI18n();
const toast = useToastStore();

type DialogEl = HTMLDialogElement & { showModal: () => void; close: () => void };
const dialogEl = ref<DialogEl | null>(null);
const book = ref<Book | null>(null);
const selected = ref<'cb7' | 'epub' | 'pdf'>('cb7');
const busy = ref(false);
/** True while the progress view is up (destination picked, export running). */
const exporting = ref(false);
const progress = ref({ done: 0, total: 0 });
/** Book whose progress events we accept — keeps stale events from other
    exports from moving this bar. */
const activeBookId = ref('');
let unlisten: UnlistenFn | null = null;

/** (done + 1) / (total + 1): the backend fires after each page is written,
    so done never reaches total — the +1 keeps the bar from stalling at 100%
    before the save promise resolves. */
const progressValue = computed(() =>
  (progress.value.done + 1) / (progress.value.total + 1),
);

const formats = [
  {
    value: 'cb7' as const,
    icon: mdiArchive,
    label: t('lib.save.format.cb7'),
    desc: '.cb7',
  },
  {
    value: 'epub' as const,
    icon: mdiBookOpen,
    label: t('lib.save.format.epub'),
    desc: '.epub',
  },
  {
    value: 'pdf' as const,
    icon: mdiFilePdfBox,
    label: t('lib.save.format.pdf'),
    desc: '.pdf',
  },
];

function open(b: Book) {
  book.value = b;
  // Default to the stored format if it's one of ours, else cb7.
  selected.value = b.format === 'cb7' || b.format === 'epub' || b.format === 'pdf'
    ? b.format
    : 'cb7';
  dialogEl.value?.showModal();
}

function close() {
  if (busy.value) return;
  dialogEl.value?.close();
  book.value = null;
}

/** Escape while exporting would close the dialog behind the running job —
    swallow it; the job finishes and auto-closes. */
function onEscape(e: Event) {
  if (busy.value) e.preventDefault();
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) close();
}

function stopListening() {
  unlisten?.();
  unlisten = null;
  activeBookId.value = '';
}

async function confirm() {
  if (!book.value || busy.value) return;
  busy.value = true;
  const b = book.value;
  const fmt = selected.value;
  const defaultName = `${b.title || 'book'}.${fmt}`;
  try {
    const dest = await dialogSave({
      defaultPath: defaultName,
      filters: [
        { name: fmt.toUpperCase(), extensions: [fmt] },
        { name: t('lib.save.allFiles'), extensions: ['*'] },
      ],
    });
    if (!dest) {
      busy.value = false;
      return;
    }

    // Listener is registered before the invoke so no page event can slip
    // past; cb7 fires one event, epub/pdf fire per page.
    exporting.value = true;
    progress.value = { done: 0, total: Math.max(b.page_count, 0) };
    activeBookId.value = b.id;
    unlisten?.();
    unlisten = await listen<ExportProgress>('book://export-progress', (event) => {
      const p = event.payload;
      if (p.book_id !== activeBookId.value) return;
      progress.value = { done: p.done, total: p.total };
    });

    await api.saveBook(b.id, dest, fmt);
    toast.addToast('success', t('lib.saved', { title: b.title }));
    // Progress finished → the dialog dismisses itself.
    dialogEl.value?.close();
    book.value = null;
  } catch (e) {
    toast.addToast('error', t('lib.saveFailed', { error: String(e) }));
  } finally {
    stopListening();
    exporting.value = false;
    busy.value = false;
  }
}

onUnmounted(stopListening);

defineExpose({ open, close });
</script>

<style scoped>
.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 40px;
  height: 40px;
  padding: 0;
  border: none;
  border-radius: var(--md-sys-shape-corner-full);
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  cursor: pointer;
  transition: background-color 0.15s ease;
}

.icon-btn:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
}

.export-dialog {
  width: min(440px, calc(100vw - 48px));
  padding: 0;
  border: none;
  border-radius: var(--md-sys-shape-corner-large);
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface);
  box-shadow: var(--md-sys-elevation-level3);
  overflow: hidden;
}
.export-dialog::backdrop {
  background: rgba(0, 0, 0, 0.4);
}

.export-dialog__panel {
  display: flex;
  flex-direction: column;
}

.export-dialog__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 16px 20px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
}

.export-dialog__title {
  font: var(--md-sys-typescale-title-large-weight)
    var(--md-sys-typescale-title-large-size) /
    var(--md-sys-typescale-title-large-line-height)
    var(--md-sys-typescale-font);
}

.export-dialog__subtitle {
  margin: 0;
  padding: 12px 20px 4px;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 14px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.format-options {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 20px 16px;
}

.format-option {
  display: grid;
  grid-template-columns: 28px 1fr auto;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border: 1px solid var(--md-sys-color-outline-variant);
  border-radius: var(--md-sys-shape-corner-medium);
  background: transparent;
  color: var(--md-sys-color-on-surface);
  cursor: pointer;
  text-align: left;
  transition: border-color 0.15s ease, background-color 0.15s ease;
}

.format-option:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 6%, transparent);
}

.format-option--selected {
  border-color: var(--md-sys-color-primary);
  background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);
}

.format-option__label {
  font-size: 14px;
  font-weight: 500;
}

.format-option__desc {
  color: var(--md-sys-color-on-surface-variant);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.export-dialog__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 8px 20px 20px;
}

.export-progress {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px 20px 24px;
}

.export-progress__label {
  margin: 0;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 13px;
  font-variant-numeric: tabular-nums;
}
</style>
