<template>
  <dialog ref="dialogEl" class="meta-dialog" @click="onBackdrop">
    <div v-if="book" class="meta-dialog__panel">
      <div class="meta-dialog__header">
        <span class="meta-dialog__title">{{ t('lib.viewMeta') }}</span>
        <button
          class="icon-btn"
          :aria-label="t('common.dismiss')"
          @click="close"
        >
          <MdiIcon :path="mdiClose" :size="20" />
        </button>
      </div>
      <dl class="meta-list">
        <template v-for="row in headRows" :key="row.label">
          <dt>{{ row.label }}</dt>
          <dd>{{ row.value }}</dd>
        </template>
        <dt>{{ t('lib.meta.tags') }}</dt>
        <dd>
          <div v-if="tagList.length" class="tag-chips">
            <span
              v-for="tag in tagList"
              :key="tag"
              class="tag-chip tag-chip--readonly"
            >{{ tag }}</span>
          </div>
          <span v-else>—</span>
        </dd>
        <template v-for="row in midRows" :key="row.label">
          <dt>{{ row.label }}</dt>
          <dd>{{ row.value }}</dd>
        </template>
        <dt>{{ t('lib.meta.sourceUrl') }}</dt>
        <dd>
          <a
            v-if="book.source_url"
            class="meta-link"
            :href="book.source_url"
            target="_blank"
            rel="noreferrer"
          >{{ book.source_url }}</a>
          <span v-else>—</span>
        </dd>
        <template v-for="row in tailRows" :key="row.label">
          <dt>{{ row.label }}</dt>
          <dd>{{ row.value }}</dd>
        </template>
      </dl>
    </div>
  </dialog>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { mdiClose } from '@mdi/js';
import { useI18n } from '@/i18n';
import { api } from '@/services/api';
import { formatSize } from '@/utils/format';
import MdiIcon from '@/components/MdiIcon.vue';
import type { Book } from '@/types';

const { t } = useI18n();

type DialogEl = HTMLDialogElement & { showModal: () => void; close: () => void };
const dialogEl = ref<DialogEl | null>(null);
const book = ref<Book | null>(null);

/** Show the dialog for a given book and re-fetch to ensure tags reflect the
 *  CURRENT locale (the grid book may be stale from a previous language). */
function open(b: Book) {
  book.value = b;
  dialogEl.value?.showModal();
  void api.getBook(b.id).then((fresh) => {
    if (book.value?.id === fresh.id) book.value = fresh;
  }).catch(() => {});
}

function close() {
  dialogEl.value?.close();
  book.value = null;
}

function onBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) close();
}

/** Format a website publish time (ISO/RFC or site-local) into a local date;
 *  tolerates partial formats like EHentai's "2024-01-15 12:00". */
function formatDate(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (!Number.isNaN(d.getTime())) return d.toLocaleDateString();
  const m = iso.match(/^\d{4}-\d{2}-\d{2}/);
  return m ? m[0] : iso;
}

/** Tags from the comma-joined DB string, split for chip display. */
const tagList = computed<string[]>(() =>
  (book.value?.tags ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0),
);

const headRows = computed(() => {
  if (!book.value) return [];
  return [
    { label: t('lib.meta.title'), value: book.value.title || '—' },
    { label: t('lib.meta.author'), value: book.value.author || '—' },
  ];
});

const midRows = computed(() => {
  if (!book.value) return [];
  return [
    { label: t('lib.meta.published'), value: formatDate(book.value.published_at) || '—' },
    { label: t('lib.meta.source'), value: book.value.source_plugin || '—' },
    { label: t('lib.meta.postId'), value: book.value.source_post_id || '—' },
  ];
});

const tailRows = computed(() => {
  if (!book.value) return [];
  return [
    { label: t('lib.meta.format'), value: (book.value.format || '').toUpperCase() || '—' },
    { label: t('lib.meta.pages'), value: String(book.value.page_count ?? 0) },
    { label: t('lib.meta.size'), value: formatSize(book.value.file_size) },
    { label: t('lib.meta.imported'), value: formatDate(book.value.created_at) },
    { label: t('lib.meta.scraped'), value: formatDate(book.value.scraped_at) || '—' },
  ];
});

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

.meta-dialog {
  width: min(480px, calc(100vw - 48px));
  max-height: calc(100vh - 96px);
  padding: 0;
  border: none;
  border-radius: var(--md-sys-shape-corner-large);
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-on-surface);
  box-shadow: var(--md-sys-elevation-level3);
  overflow: hidden;
}
.meta-dialog::backdrop {
  background: rgba(0, 0, 0, 0.4);
}

.meta-dialog__panel {
  display: flex;
  flex-direction: column;
  max-height: calc(100vh - 96px);
}

.meta-dialog__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 16px 20px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);
}

.meta-dialog__title {
  font: var(--md-sys-typescale-title-large-weight)
    var(--md-sys-typescale-title-large-size) /
    var(--md-sys-typescale-title-large-line-height)
    var(--md-sys-typescale-font);
}

.meta-list {
  margin: 0;
  padding: 8px 20px 20px;
  overflow-y: auto;
  display: grid;
  grid-template-columns: max-content 1fr;
  column-gap: 24px;
  row-gap: 8px;
}

.meta-list dt {
  color: var(--md-sys-color-on-surface-variant);
  font: var(--md-sys-typescale-body-medium-weight)
    var(--md-sys-typescale-body-medium-size) /
    var(--md-sys-typescale-body-medium-line-height)
    var(--md-sys-typescale-font);
  white-space: nowrap;
}

.meta-list dd {
  margin: 0;
  font: var(--md-sys-typescale-body-medium-weight)
    var(--md-sys-typescale-body-medium-size) /
    var(--md-sys-typescale-body-medium-line-height)
    var(--md-sys-typescale-font);
  word-break: break-all;
}

.meta-link {
  color: var(--md-sys-color-primary);
  text-decoration: none;
  word-break: break-all;
}

.meta-link:hover {
  text-decoration: underline;
}

.tag-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 26px;
  padding: 0 10px;
  border: 1px solid var(--md-sys-color-outline);
  border-radius: var(--md-sys-shape-corner-full);
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  font-size: 12px;
  line-height: 1;
}

.tag-chip--readonly {
  cursor: default;
  pointer-events: none;
}
</style>
