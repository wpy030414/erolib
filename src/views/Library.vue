<template>
  <div class="pa-6">
    <div class="library-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 library-header__title">{{ t('nav.library') }}</h2>
      <span class="spacer" />

      <md-outlined-text-field
        class="header-search"
        :value="libraryStore.query"
        :label="t('lib.search.placeholder')"
        @input="libraryStore.query = ($event.target as HTMLInputElement).value"
      >
        <MdiIcon slot="leading-icon" :path="mdiMagnify" :size="18" />
      </md-outlined-text-field>

      <md-filled-button @click="onImport">
        <MdiIcon slot="icon" :path="mdiFolderOpen" :size="20" />
        {{ t('lib.import') }}
      </md-filled-button>

      <md-circular-progress v-if="libraryStore.isLoading" indeterminate />
    </div>

    <div v-if="libraryStore.allTags.length" class="tag-chips mb-6">
      <button
        v-for="tag in libraryStore.allTags"
        :key="tag.name"
        class="tag-chip"
        :class="{ 'tag-chip--selected': libraryStore.selectedTags.includes(tag.name) }"
        :aria-pressed="libraryStore.selectedTags.includes(tag.name)"
        @click="libraryStore.toggleTag(tag.name)"
      >
        <span class="tag-chip__label">{{ tag.name }}</span>
        <span class="tag-chip__count">({{ tag.count }})</span>
      </button>
      <!-- The chip row is capped (backend returns the top 30 tags). When that
           cap is reached, show a non-interactive "…" chip to signal more tags
           exist — they're reachable via the search box, which also matches tag
           names now. -->
      <span
        v-if="libraryStore.allTags.length >= TAG_DISPLAY_LIMIT"
        class="tag-chip tag-chip--ellipsis"
        aria-hidden="true"
      >…</span>
    </div>

    <div
      v-if="libraryStore.books.length"
      class="md3-grid"
    >
      <div v-for="book in libraryStore.books" :key="book.id">
        <div
          :id="'book-anchor-' + book.id"
          class="md3-card md3-card--outlined book-card"
          @click="router.push(`/reader/${book.id}`)"
          @contextmenu.prevent="openMenu(book.id)"
        >
          <div class="book-cover-wrap">
            <img
              v-if="coverMap[book.id]"
              :src="coverMap[book.id]!"
              class="book-cover"
              :alt="book.title"
            />
            <div v-else class="book-placeholder">
              {{ book.title.charAt(0).toUpperCase() }}
            </div>
            <div class="book-pages-badge">{{ book.page_count }}</div>
          </div>

          <div class="md3-card__content">
            <div class="md3-card__title text-truncate text-subtitle-2">
              {{ book.title }}
            </div>
            <div
              v-if="book.author"
              class="md3-card__subtitle text-body-2 text-truncate"
            >
              {{ book.author }}
            </div>
          </div>
        </div>

        <md-menu
          :id="'book-menu-' + book.id"
          :ref="(el: unknown) => setMenuRef(book.id, el as MdMenuElement | null)"
          :anchor="'book-anchor-' + book.id"
          :open="menuOpen[book.id]"
          positioning="fixed"
          @closed="menuOpen[book.id] = false"
        >
          <md-menu-item @click="viewMeta(book)">
            <MdiIcon slot="start" :path="mdiInformationOutline" :size="18" />
            <div slot="headline">{{ t('lib.viewMeta') }}</div>
          </md-menu-item>
          <md-menu-item @click="saveToLocal(book)">
            <MdiIcon slot="start" :path="mdiContentSave" :size="18" />
            <div slot="headline">{{ t('lib.save') }}</div>
          </md-menu-item>
          <md-menu-item @click="deleteBookItem(book)">
            <MdiIcon slot="start" :path="mdiDelete" :size="18" />
            <div slot="headline">{{ t('lib.delete') }}</div>
          </md-menu-item>
        </md-menu>
      </div>
    </div>

    <div
      v-else-if="!libraryStore.isLoading"
      class="text-center text-medium-emphasis mt-8"
    >
      {{ t('lib.empty') }}
    </div>

    <dialog ref="metaDialog" class="meta-dialog" @click="onDialogBackdrop">
      <div v-if="metaBook" class="meta-dialog__panel">
        <div class="meta-dialog__header">
          <span class="meta-dialog__title">{{ t('lib.viewMeta') }}</span>
          <button
            class="icon-btn"
            :aria-label="t('common.dismiss')"
            @click="closeMeta"
          >
            <MdiIcon :path="mdiClose" :size="20" />
          </button>
        </div>
        <dl class="meta-list">
          <template v-for="row in metaRows(metaBook)" :key="row.label">
            <dt>{{ row.label }}</dt>
            <dd>{{ row.value }}</dd>
          </template>
        </dl>
      </div>
    </dialog>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, reactive } from 'vue';
import { useRouter } from 'vue-router';
import { save as dialogSave } from '@tauri-apps/plugin-dialog';
import {
  mdiMagnify,
  mdiFolderOpen,
  mdiContentSave,
  mdiDelete,
  mdiInformationOutline,
  mdiClose,
} from '@mdi/js';
import { useLibraryStore } from '@/stores/library';
import { api } from '@/services/api';
import { getThumb, setThumb, deleteThumb } from '@/services/thumb-cache';
import { useI18n } from '@/i18n';
import MdiIcon from '@/components/MdiIcon.vue';
import type { Book } from '@/types';

type MdMenuElement = HTMLElement & {
  show: () => void;
  close: () => void;
  open: boolean;
};

const router = useRouter();
const libraryStore = useLibraryStore();
const { t } = useI18n();

const coverMap = reactive<Record<string, string | null>>({});
const menuOpen = reactive<Record<string, boolean>>({});
const menuRefs = new Map<string, MdMenuElement | null>();

let searchTimer: ReturnType<typeof setTimeout> | null = null;
const SEARCH_DEBOUNCE_MS = 300;
/** Chip-row display cap (backend `get_all_tags` returns the top 30). When the
 *  cap is reached we append a non-interactive "…" chip to signal more exist. */
const TAG_DISPLAY_LIMIT = 30;

let prevIds = new Set<string>();

function setMenuRef(bookId: string, el: MdMenuElement | null) {
  if (el) {
    menuRefs.set(bookId, el);
  } else {
    menuRefs.delete(bookId);
  }
}

function openMenu(bookId: string) {
  menuOpen[bookId] = true;
  const menuEl = menuRefs.get(bookId);
  if (menuEl && typeof menuEl.show === 'function') {
    menuEl.show();
  }
}

async function loadCover(book: Book) {
  if (book.id in coverMap) return;
  coverMap[book.id] = null;
  let alive = true;
  let made: string | null = null;
  try {
    // Try the IndexedDB thumbnail cache first (instant, no IPC); on miss fetch
    // a low-res thumb from the backend and cache it for next time.
    let blob = await getThumb(book.id);
    if (!blob) {
      const bytes = await api.getBookCoverThumb(book.id);
      if (!alive) return;
      blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
      void setThumb(book.id, blob);
    }
    if (!alive) return;
    made = URL.createObjectURL(blob);
    coverMap[book.id] = made;
  } catch {
    // leave placeholder
  }
  return () => {
    alive = false;
    if (made) URL.revokeObjectURL(made);
  };
}

const stopWatch = watch(
  () => libraryStore.books,
  (books) => {
    const currentIds = new Set(books.map((b) => b.id));
    for (const id of prevIds) {
      if (!currentIds.has(id)) {
        const url = coverMap[id];
        if (url) URL.revokeObjectURL(url);
        delete coverMap[id];
        delete menuOpen[id];
        menuRefs.delete(id);
      }
    }
    prevIds = currentIds;
    for (const book of books) loadCover(book);
  },
  { immediate: true },
);

onMounted(() => {
  libraryStore.ensureLoaded();
});

onBeforeUnmount(() => {
  stopWatch();
  if (searchTimer) clearTimeout(searchTimer);
  for (const url of Object.values(coverMap)) {
    if (url) URL.revokeObjectURL(url);
  }
  menuRefs.clear();
});

/** Format a website publish time (ISO/RFC or site-local) into a local date;
 *  tolerates partial formats like EHentai's "2024-01-15 12:00". */
function formatDate(iso?: string): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (!Number.isNaN(d.getTime())) return d.toLocaleDateString();
  const m = iso.match(/^\d{4}-\d{2}-\d{2}/);
  return m ? m[0] : iso;
}

watch(
  () => libraryStore.query,
  () => {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      libraryStore.applySearch();
    }, SEARCH_DEBOUNCE_MS);
  },
);

async function onImport() {
  const file = await api.openFile([
    { name: t('lib.import.filterName'), extensions: ['cb7', 'cbz', 'cbr', 'pdf'] },
  ]);
  if (typeof file === 'string') {
    await api.importBook(file);
    await libraryStore.refresh();
  }
}

async function deleteBookItem(book: Book) {
  menuOpen[book.id] = false;
  try {
    await libraryStore.deleteBook(book.id);
    void deleteThumb(book.id);
  } catch (e) {
    // eslint-disable-next-line no-console
    console.error(t('common.error', { message: String(e) }));
  }
}

async function saveToLocal(book: Book) {
  menuOpen[book.id] = false;
  const defaultName = `${book.title || 'book'}.${book.format}`;
  const dest = await dialogSave({
    defaultPath: defaultName,
    filters: [
      { name: t('lib.save.filterName'), extensions: [book.format] },
      { name: t('lib.save.allFiles'), extensions: ['*'] },
    ],
  });
  if (dest) {
    await api.saveBook(book.id, dest);
  }
}

const metaDialog = ref<HTMLDialogElement | null>(null);
const metaBook = ref<Book | null>(null);

function viewMeta(book: Book) {
  menuOpen[book.id] = false;
  metaBook.value = book;
  metaDialog.value?.showModal();
}

function closeMeta() {
  metaDialog.value?.close();
}

function onDialogBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) closeMeta();
}

/** Human-readable byte size, e.g. "12.3 MB". */
function formatSize(bytes?: number): string {
  if (!bytes) return '—';
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** Ordered label/value rows shown in the metadata dialog. */
function metaRows(book: Book): { label: string; value: string }[] {
  return [
    { label: t('lib.meta.title'), value: book.title || '—' },
    { label: t('lib.meta.author'), value: book.author || '—' },
    { label: t('lib.meta.source'), value: book.source_plugin || '—' },
    { label: t('lib.meta.postId'), value: book.source_post_id || '—' },
    { label: t('lib.meta.published'), value: formatDate(book.published_at) || '—' },
    { label: t('lib.meta.pages'), value: String(book.page_count ?? 0) },
    { label: t('lib.meta.format'), value: (book.format || '').toUpperCase() || '—' },
    { label: t('lib.meta.size'), value: formatSize(book.file_size) },
    { label: t('lib.meta.tags'), value: book.tags || '—' },
    { label: t('lib.meta.sourceUrl'), value: book.source_url || '—' },
    { label: t('lib.meta.imported'), value: formatDate(book.created_at) },
    { label: t('lib.meta.scraped'), value: formatDate(book.scraped_at) || '—' },
  ];
}
</script>

<style scoped>
.library-header__title {
  margin: 0;
  white-space: nowrap;
}

.header-search {
  --md-outlined-text-field-container-height: 40px;
  width: 260px;
  flex: 0 0 260px;
}

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
  cursor: pointer;
  transition:
    background-color 0.15s ease,
    color 0.15s ease,
    border-color 0.15s ease;
}

.tag-chip:hover {
  background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
}

.tag-chip--selected {
  background: var(--md-sys-color-secondary-container);
  border-color: transparent;
  color: var(--md-sys-color-on-secondary-container);
}

.tag-chip--selected:hover {
  background: color-mix(
    in srgb,
    var(--md-sys-color-on-secondary-container) 12%,
    var(--md-sys-color-secondary-container)
  );
}

.tag-chip__count {
  font-size: 11px;
  opacity: 0.75;
}

/* Non-interactive ellipsis chip shown when the chip row hits its cap. */
.tag-chip--ellipsis {
  border: none;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  cursor: default;
  opacity: 0.6;
}
</style>
