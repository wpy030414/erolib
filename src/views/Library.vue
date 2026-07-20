<template>
  <div class="pa-6">
    <div class="library-header d-flex align-center gap-4 mb-6">
      <h2 class="text-h5 library-header__title">
        <template v-if="collectionsStore.isAllActive">{{ t('nav.library') }}</template>
        <template v-else>"{{ collectionsStore.activeCollectionName }}"</template>
      </h2>
      <span class="spacer" />

      <SearchBox
        v-model="libraryStore.query"
        :placeholder="t('lib.search.placeholder')"
        :clear-label="t('common.clear')"
        @commit="libraryStore.applySearch"
      />

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
        <SourceCard
          :id="'book-anchor-' + book.id"
          :title="book.title"
          :page-count="book.page_count"
          :subtitle="book.author"
          :cover="coverMap[book.id] ?? null"
          @click="router.push(`/reader/${book.id}`)"
          @contextmenu.prevent="openMenu(book.id)"
        />

        <md-menu
          :id="'book-menu-' + book.id"
          :ref="(el: unknown) => setMenuRef(book.id, el as MdMenuElement | null)"
          :anchor="'book-anchor-' + book.id"
          :open="menuOpen[book.id]"
          positioning="fixed"
          @closed="menuOpen[book.id] = false"
        >
          <md-menu-item @click="openCollectionPicker(book.id)">
            <MdiIcon slot="start" :path="mdiPlaylistPlus" :size="18" />
            <div slot="headline">{{ t('lib.collections.addTo') }}</div>
          </md-menu-item>
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

    <div v-if="libraryStore.books.length" class="feed-sentinel-wrap">
      <!-- Infinite-scroll sentinel: intersecting triggers loadMore(); the store
           no-ops while a load is in flight or the filter is exhausted. -->
      <div ref="sentinelEl" class="feed-sentinel" />
      <div v-if="libraryStore.isLoadingMore" class="feed-loading">
        <md-circular-progress indeterminate />
      </div>
    </div>

    <div
      v-if="!libraryStore.books.length && !libraryStore.isLoading"
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
          <dt>{{ t('lib.meta.tags') }}</dt>
          <dd>
            <div v-if="metaTags.length" class="tag-chips">
              <span
                v-for="tag in metaTags"
                :key="tag"
                class="tag-chip tag-chip--readonly"
              >{{ tag }}</span>
            </div>
            <span v-else>—</span>
          </dd>
          <template v-for="row in metaRowsMid(metaBook)" :key="row.label">
            <dt>{{ row.label }}</dt>
            <dd>{{ row.value }}</dd>
          </template>
          <dt>{{ t('lib.meta.sourceUrl') }}</dt>
          <dd>
            <a
              v-if="metaBook?.source_url"
              class="meta-link"
              :href="metaBook!.source_url"
              target="_blank"
              rel="noreferrer"
            >{{ metaBook!.source_url }}</a>
            <span v-else>—</span>
          </dd>
          <template v-for="row in metaRowsAfter(metaBook)" :key="row.label">
            <dt>{{ row.label }}</dt>
            <dd>{{ row.value }}</dd>
          </template>
        </dl>
      </div>
    </dialog>

    <!-- Collection management FAB + dialogs -->
    <FabButton
      :icon="mdiPlaylistPlay"
      :aria-label="t('lib.collections.manage')"
      @click="showCollectionDialog = true"
    />

    <CollectionDialog
      :model-value="showCollectionDialog"
      @update:model-value="showCollectionDialog = $event"
      @close="showCollectionDialog = false"
    />

    <BookCollectionPicker
      v-if="pickerBookId"
      :book-id="pickerBookId"
      @close="pickerBookId = null"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, reactive } from 'vue';
import { useRouter, useRoute } from 'vue-router';
import { save as dialogSave } from '@tauri-apps/plugin-dialog';
import {
  mdiFolderOpen,
  mdiContentSave,
  mdiDelete,
  mdiInformationOutline,
  mdiClose,
  mdiPlaylistPlay,
  mdiPlaylistPlus,
} from '@mdi/js';
import { useLibraryStore } from '@/stores/library';
import { useCollectionsStore } from '@/stores/collections';
import { api } from '@/services/api';
import { getThumb, setThumb, deleteThumb } from '@/services/thumb-cache';
import { useToastStore } from '@/stores/toast';
import { useI18n } from '@/i18n';
import MdiIcon from '@/components/MdiIcon.vue';
import SourceCard from '@/components/SourceCard.vue';
import SearchBox from '@/components/SearchBox.vue';
import FabButton from '@/components/FabButton.vue';
import CollectionDialog from '@/components/CollectionDialog.vue';
import BookCollectionPicker from '@/components/BookCollectionPicker.vue';
import { useInfiniteSentinel } from '@/composables/useInfiniteSentinel';
import { formatSize } from '@/utils/format';
import type { Book } from '@/types';

type MdMenuElement = HTMLElement & {
  show: () => void;
  close: () => void;
  open: boolean;
};

const router = useRouter();
const route = useRoute();
const libraryStore = useLibraryStore();
const collectionsStore = useCollectionsStore();
const toast = useToastStore();
const { t } = useI18n();

/** Infinite-scroll sentinel — IntersectionObserver calls loadMore() when the
 *  grid bottom scrolls near (the store no-ops while busy or exhausted). */
const sentinelEl = ref<HTMLElement | null>(null);
useInfiniteSentinel(sentinelEl, () => libraryStore.loadMore());

const coverMap = reactive<Record<string, string | null>>({});
const menuOpen = reactive<Record<string, boolean>>({});
const menuRefs = new Map<string, MdMenuElement | null>();
const showCollectionDialog = ref(false);
const pickerBookId = ref<string | null>(null);

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

function openCollectionPicker(bookId: string) {
  menuOpen[bookId] = false;
  pickerBookId.value = bookId;
}

async function loadCover(book: Book) {
  if (book.id in coverMap) return;
  coverMap[book.id] = null;
  let alive = true;
  let made: string | null = null;
  try {
    // Try the IndexedDB thumbnail cache first. Key by source_post_id (Pixiv
    // illust id / EHentai gid) so the SAME cover is shared with the browse
    // pages — a cover cached while browsing is reused for the downloaded book
    // and vice versa. Local imports (no source_post_id) fall back to book id.
    const cacheKey = book.source_post_id || book.id;
    let blob = await getThumb(cacheKey);
    if (!blob) {
      const bytes = await api.getBookCoverThumb(book.id);
      if (!alive) return;
      blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
      void setThumb(cacheKey, blob);
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

const stopCoverWatch = watch(
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
  collectionsStore.ensureLoaded();
});

// When the active collection changes, re-filter the library and re-tally tags.
const stopCollectionWatch = watch(
  () => collectionsStore.activeCollectionId,
  () => {
    libraryStore.collectionFilter = collectionsStore.activeCollectionName;
    // reload() fetches books, but we also need tag counts scoped to the
    // collection. applySearch() calls loadTags() with the current text filter.
    libraryStore.applySearch();
  },
);

// When navigated to (e.g. from Tasks "view" button) with ?search=…,
// set the text box and trigger a search. Also sync the collection filter
// so the switch to "All" (done by Tasks before navigating) takes effect.
const stopRouteWatch = watch(
  () => route.query.search,
  (val) => {
    const text = (typeof val === 'string' && val) ? val : '';
    if (!text) return;
    // Always sync collectionFilter to the current store state first, so a
    // prior setActiveCollection(null) from another view takes effect.
    libraryStore.collectionFilter = collectionsStore.activeCollectionName;
    if (text === libraryStore.query) {
      // Same query text but collection may have changed — still reload.
      libraryStore.applySearch();
      return;
    }
    libraryStore.query = text;
    libraryStore.applySearch();
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  stopCoverWatch();
  stopRouteWatch();
  stopCollectionWatch();
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

async function onImport() {
  const file = await api.openFile([
    { name: t('lib.import.filterName'), extensions: ['cb7', 'cbz', 'cbr', 'pdf'] },
  ]);
  if (typeof file === 'string') {
    try {
      const book = await api.importBook(file);
      await libraryStore.refresh();
      toast.addToast('success', t('lib.imported', { title: book.title }));
    } catch (e) {
      toast.addToast('error', t('lib.importFailed', { error: String(e) }));
    }
  }
}

async function deleteBookItem(book: Book) {
  menuOpen[book.id] = false;
  try {
    await libraryStore.deleteBook(book.id);
    void deleteThumb(book.id);
    toast.addToast('success', t('lib.deleted', { title: book.title }));
  } catch (e) {
    toast.addToast('error', t('lib.deleteFailed', { error: String(e) }));
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
    try {
      await api.saveBook(book.id, dest);
      toast.addToast('success', t('lib.saved', { title: book.title }));
    } catch (e) {
      toast.addToast('error', t('lib.saveFailed', { error: String(e) }));
    }
  }
}

const metaDialog = ref<HTMLDialogElement | null>(null);
const metaBook = ref<Book | null>(null);

function viewMeta(book: Book) {
  menuOpen[book.id] = false;
  metaBook.value = book;
  metaDialog.value?.showModal();
  // Re-fetch so the tags reflect the CURRENT locale: the grid `book` is a stale
  // copy rendered under the previous language, while get_book translates fresh.
  void api.getBook(book.id).then((fresh) => {
    if (metaBook.value?.id === fresh.id) metaBook.value = fresh;
  }).catch(() => {});
}

function closeMeta() {
  metaDialog.value?.close();
}

function onDialogBackdrop(e: MouseEvent) {
  if (e.target === e.currentTarget) closeMeta();
}

/** Tags from the comma-joined DB string, split for chip display. */
const metaTags = computed<string[]>(() =>
  (metaBook.value?.tags ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0),
);

/** Meta rows before tags (title, author). */
function metaRows(book: Book): { label: string; value: string }[] {
  return [
    { label: t('lib.meta.title'), value: book.title || '—' },
    { label: t('lib.meta.author'), value: book.author || '—' },
  ];
}

/** Meta rows between tags and sourceUrl (published, source, postId). */
function metaRowsMid(book: Book): { label: string; value: string }[] {
  return [
    { label: t('lib.meta.published'), value: formatDate(book.published_at) || '—' },
    { label: t('lib.meta.source'), value: book.source_plugin || '—' },
    { label: t('lib.meta.postId'), value: book.source_post_id || '—' },
  ];
}

/** Meta rows after sourceUrl (format, pages, size, imported, scraped). */
function metaRowsAfter(book: Book): { label: string; value: string }[] {
  return [
    { label: t('lib.meta.format'), value: (book.format || '').toUpperCase() || '—' },
    { label: t('lib.meta.pages'), value: String(book.page_count ?? 0) },
    { label: t('lib.meta.size'), value: formatSize(book.file_size) },
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

/* Read-only chips in the metadata dialog — same look, no interaction. */
.tag-chip--readonly {
  cursor: default;
  pointer-events: none;
}

/* Non-interactive ellipsis chip shown when the chip row hits its cap. */
.tag-chip--ellipsis {
  border: none;
  background: transparent;
  color: var(--md-sys-color-on-surface-variant);
  cursor: default;
  opacity: 0.6;
}

/* Infinite-scroll sentinel: a 1px observer target at the grid bottom; the
 * IntersectionObserver in useInfiniteSentinel watches it. */
.feed-sentinel {
  height: 1px;
  width: 100%;
}

.feed-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px 0;
}
</style>
