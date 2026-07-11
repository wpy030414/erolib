import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { api } from '@/services/api';
import type { Book, SearchQuery, SearchResult, TagCount } from '@/types';
import { useSettingsStore } from './settings';

/** Page size for the library grid's infinite scroll. The backend search clamps
 *  page_size to 1..200 and returns `total`, so the grid can tell when the
 *  current text+tag filter is exhausted. 48 matches the browse feeds' unified
 *  page size (useBrowseFeed) so local + Pixiv + EHentai all grow at the same
 *  cadence. */
const PAGE_SIZE = 48;

export const useLibraryStore = defineStore('library', () => {
  const books = ref<Book[]>([]);
  const isLoading = ref(false);
  /** Loading a subsequent page (not the first) — drives the bottom "loading
   *  more" indicator without flashing the empty state. */
  const isLoadingMore = ref(false);
  const error = ref<string | null>(null);
  const query = ref('');
  /** Tags currently selected in the chip row — union (OR) filter: any match. */
  const selectedTags = ref<string[]>([]);
  /** Tag usage counts that drive the chip row. When a text query is active
   *  these are tallied only over the text-filtered book set (text dominates
   *  the chips); otherwise over the full library. Top 30 by count. */
  const allTags = ref<TagCount[]>([]);
  /** Whether the initial load has happened. Keeps view-switches from
   *  resetting the current search / filter state and results. */
  const initialized = ref(false);

  /** Total matching books for the current text+tag filter (backend search
   *  `total`), used to gate infinite scroll. */
  const total = ref(0);
  const page = ref(1);
  const hasMore = computed(() => books.value.length < total.value);

  /** Re-tally tag usage counts. `text` restricts the count to books matching
   *  that text (text dominates the chips). Capped to 30 by the backend.
   *  Silent on failure. */
  async function loadTags(text?: string) {
    try {
      allTags.value = await api.getAllTags(text);
    } catch {
      // keep the previous list on error
    }
  }

  /** Fetch one page of the current text+tag filter. `accumulate` appends
   *  (infinite scroll) vs replaces (new search / filter change). */
  async function fetchPage(p: number, accumulate: boolean) {
    const text = query.value.trim();
    const tagsAny = selectedTags.value.length ? [...selectedTags.value] : undefined;
    const q: SearchQuery = {
      text: text || undefined,
      tags_any: tagsAny,
      sort_by: 'date',
      sort_order: 'desc',
      page: p,
      page_size: PAGE_SIZE,
    };
    const res: SearchResult = await api.searchBooks(q);
    books.value = accumulate ? [...books.value, ...res.books] : res.books;
    total.value = res.total ?? 0;
    page.value = p;
  }

  /** (Re)load page 1 — used on init, search, tag toggle, import, delete. */
  async function reload() {
    isLoading.value = true;
    error.value = null;
    try {
      await fetchPage(1, false);
    } catch (e) {
      error.value = String(e);
    } finally {
      isLoading.value = false;
    }
  }

  /** Load the next page, appending to the grid. No-op while busy or when the
   *  current filter is exhausted. */
  async function loadMore() {
    if (isLoading.value || isLoadingMore.value || !hasMore.value) return;
    isLoadingMore.value = true;
    try {
      await fetchPage(page.value + 1, true);
    } catch (e) {
      error.value = String(e);
    } finally {
      isLoadingMore.value = false;
    }
  }

  /** Refresh books and tag counts together (full library, ignoring query). */
  async function refresh() {
    await Promise.all([reload(), loadTags()]);
  }

  /** Load the library once; later calls (e.g. after switching away and back
   *  to the view) are no-ops, so search / filter state is preserved. */
  async function ensureLoaded() {
    if (initialized.value) return;
    initialized.value = true;
    await refresh();
  }

  /** Text changed: re-tally tags under the new text (so the chip counts and
   *  the chip set reflect the text results), drop any selection whose tag
   *  vanished from those results, then reload page 1. */
  async function applySearch() {
    const text = query.value.trim();
    await loadTags(text || undefined);
    if (selectedTags.value.length) {
      const present = new Set(allTags.value.map((t) => t.name));
      selectedTags.value = selectedTags.value.filter((n) => present.has(n));
    }
    await reload();
  }

  /** Toggle a tag, then reload page 1. Text is unchanged so tag counts are not
   *  re-tallied (the OR filter never affects counts). */
  function toggleTag(name: string) {
    const i = selectedTags.value.indexOf(name);
    if (i >= 0) {
      selectedTags.value.splice(i, 1);
    } else {
      selectedTags.value.push(name);
    }
    void reload();
  }

  async function importBook(filePath: string) {
    await api.importBook(filePath);
    await applySearch();
    // Local one-way sync picks up the new book (no-op unless enabled).
    void useSettingsStore().syncIfEnabled();
  }

  async function deleteBook(id: string) {
    await api.deleteBook(id);
    await applySearch();
    // Sync is add-only by design: deleting a book does NOT remove its synced
    // copy from the target directory (user's local files are never deleted).
  }

  return {
    books,
    isLoading,
    isLoadingMore,
    error,
    query,
    selectedTags,
    allTags,
    total,
    hasMore,
    refresh,
    ensureLoaded,
    applySearch,
    reload,
    loadMore,
    toggleTag,
    loadTags,
    importBook,
    deleteBook,
  };
});
