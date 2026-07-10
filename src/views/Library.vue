<template>
  <div class="pa-6">
    <h2 class="text-h5 mb-6">{{ t('nav.library') }}</h2>

    <div class="d-flex align-start flex-wrap gap-4 mb-6">
      <md-outlined-text-field
        :value="query"
        :label="t('lib.search.placeholder')"
        style="max-width: 320px; flex: 1"
        @input="query = ($event.target as HTMLInputElement).value"
      >
        <MdiIcon slot="leading-icon" :path="mdiMagnify" :size="20" />
      </md-outlined-text-field>

      <span class="spacer" />

      <button
        class="icon-btn"
        :aria-label="t('lib.refresh')"
        @click="onRefresh"
      >
        <svg
          :width="22"
          :height="22"
          viewBox="0 0 24 24"
          aria-hidden="true"
          focusable="false"
          fill="currentColor"
        >
          <path :d="mdiRefresh" />
        </svg>
      </button>

      <md-filled-button @click="onImport">
        <MdiIcon slot="icon" :path="mdiFolderOpen" :size="20" />
        {{ t('lib.import') }}
      </md-filled-button>

      <md-circular-progress v-if="libraryStore.isLoading" indeterminate />
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
          </div>

          <div class="md3-card__content">
            <div class="md3-card__title text-truncate text-subtitle-2">
              {{ book.title }}
            </div>
            <div class="md3-card__subtitle text-body-2">
              {{ t('lib.pages', { count: book.page_count }) }}
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
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, reactive } from 'vue';
import { useRouter } from 'vue-router';
import { save as dialogSave } from '@tauri-apps/plugin-dialog';
import {
  mdiMagnify,
  mdiFolderOpen,
  mdiRefresh,
  mdiContentSave,
  mdiDelete,
} from '@mdi/js';
import { useLibraryStore } from '@/stores/library';
import { api } from '@/services/api';
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

const query = ref('');
const coverMap = reactive<Record<string, string | null>>({});
const menuOpen = reactive<Record<string, boolean>>({});
const menuRefs = new Map<string, MdMenuElement | null>();

let searchTimer: ReturnType<typeof setTimeout> | null = null;
const SEARCH_DEBOUNCE_MS = 300;

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
    const bytes = await api.getBookCover(book.id);
    if (!alive) return;
    const blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
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
  libraryStore.refresh();
});

onBeforeUnmount(() => {
  stopWatch();
  if (searchTimer) clearTimeout(searchTimer);
  for (const url of Object.values(coverMap)) {
    if (url) URL.revokeObjectURL(url);
  }
  menuRefs.clear();
});

function onSearch() {
  libraryStore.search(query.value);
}

async function scheduleSearch() {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    libraryStore.search(query.value);
  }, SEARCH_DEBOUNCE_MS);
}

watch(query, () => {
  scheduleSearch();
});

async function onImport() {
  const file = await api.openFile([
    { name: t('lib.import.filterName'), extensions: ['cb7', 'cbz', 'cbr', 'pdf'] },
  ]);
  if (typeof file === 'string') {
    await api.importBook(file);
    await libraryStore.refresh();
  }
}

async function onRefresh() {
  query.value = '';
  if (searchTimer) clearTimeout(searchTimer);
  await libraryStore.refresh();
}

async function deleteBookItem(book: Book) {
  menuOpen[book.id] = false;
  try {
    await libraryStore.deleteBook(book.id);
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
</script>

<style scoped>
.spacer {
  flex: 1 1 auto;
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

.book-card {
  cursor: pointer;
  transition:
    transform 0.15s cubic-bezier(0.2, 0, 0, 1),
    box-shadow 0.15s cubic-bezier(0.2, 0, 0, 1);
}

.book-card:hover {
  transform: translateY(-2px);
  box-shadow: var(--md-sys-elevation-level1);
}

.book-cover-wrap {
  aspect-ratio: 1 / 1;
  background: var(--md-sys-color-surface-container-highest);
  overflow: hidden;
}

.book-cover {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.book-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 2.5rem;
  font-weight: bold;
  color: var(--md-sys-color-primary);
  user-select: none;
}

.md3-card__content {
  padding: 12px;
}

.md3-card__title {
  color: var(--md-sys-color-on-surface);
  font: var(--md-sys-typescale-title-small-weight)
    var(--md-sys-typescale-title-small-size) /
    var(--md-sys-typescale-title-small-line-height)
    var(--md-sys-typescale-font);
}

.md3-card__subtitle {
  color: var(--md-sys-color-on-surface-variant);
  margin-top: 4px;
  font: var(--md-sys-typescale-body-medium-weight)
    var(--md-sys-typescale-body-medium-size) /
    var(--md-sys-typescale-body-medium-line-height)
    var(--md-sys-typescale-font);
}
</style>
