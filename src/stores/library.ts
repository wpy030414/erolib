import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/services/api';
import type { Book, SearchQuery, SearchResult, TagCount } from '@/types';

export const useLibraryStore = defineStore('library', () => {
  const books = ref<Book[]>([]);
  const isLoading = ref(false);
  const error = ref<string | null>(null);
  const query = ref('');
  /** Tags currently selected in the chip row — union (OR) filter: any match. */
  const selectedTags = ref<string[]>([]);
  /** Tag usage counts that drive the chip row. When a text query is active
   *  these are tallied only over the text-filtered book set (text dominates
   *  the chips); otherwise over the full library. Top 30 by count. */
  const allTags = ref<TagCount[]>([]);
  /** Whether the initial full load has happened. Keeps view-switches from
   *  resetting the current search / filter state and results. */
  const initialized = ref(false);

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

  /** List every book (no filters), newest import first. */
  async function listAll() {
    isLoading.value = true;
    error.value = null;
    try {
      books.value = await api.listBooks();
    } catch (e) {
      error.value = String(e);
    } finally {
      isLoading.value = false;
    }
  }

  /** Refresh books and tag counts together (full library, ignoring query). */
  async function refresh() {
    await Promise.all([listAll(), loadTags()]);
  }

  /** Load the library once; later calls (e.g. after switching away and back
   *  to the view) are no-ops, so search / filter state is preserved. */
  async function ensureLoaded() {
    if (initialized.value) return;
    initialized.value = true;
    await refresh();
  }

  /** Search books using the current text + selected tags (union / OR). */
  async function runSearch() {
    const text = query.value.trim();
    const tagsAny = selectedTags.value.length ? [...selectedTags.value] : undefined;
    if (!text && !tagsAny) {
      await listAll();
      return;
    }
    isLoading.value = true;
    error.value = null;
    try {
      const q: SearchQuery = {
        text: text || undefined,
        tags_any: tagsAny,
        sort_by: 'date',
        sort_order: 'desc',
        page: 0,
        page_size: 100,
      };
      const res: SearchResult = await api.searchBooks(q);
      books.value = res.books;
    } catch (e) {
      error.value = String(e);
    } finally {
      isLoading.value = false;
    }
  }

  /** Text changed: re-tally tags under the new text (so the chip counts and
   *  the chip set reflect the text results), drop any selection whose tag
   *  vanished from those results, then search. */
  async function applySearch() {
    const text = query.value.trim();
    await loadTags(text || undefined);
    if (selectedTags.value.length) {
      const present = new Set(allTags.value.map((t) => t.name));
      selectedTags.value = selectedTags.value.filter((n) => present.has(n));
    }
    await runSearch();
  }

  /** Toggle a tag, then re-search. Text is unchanged so tag counts are not
   *  re-tallied (the OR filter never affects counts). */
  function toggleTag(name: string) {
    const i = selectedTags.value.indexOf(name);
    if (i >= 0) {
      selectedTags.value.splice(i, 1);
    } else {
      selectedTags.value.push(name);
    }
    void runSearch();
  }

  async function importBook(filePath: string) {
    await api.importBook(filePath);
    await applySearch();
  }

  async function deleteBook(id: string) {
    await api.deleteBook(id);
    await applySearch();
  }

  return {
    books,
    isLoading,
    error,
    query,
    selectedTags,
    allTags,
    refresh,
    ensureLoaded,
    listAll,
    applySearch,
    runSearch,
    toggleTag,
    loadTags,
    importBook,
    deleteBook,
  };
});
