import { create } from 'zustand';
import { api } from '@/services/api';
import type { Book, TagCount, SearchQuery } from '@/types';
import { useSettingsStore } from './settings';

const PAGE_SIZE = 48;

/** Selected chips are keyed by their (translated) display `name`; expand each
 *  to its folded raw spellings (`raw_names`) so the backend OR-matches every
 *  raw form of that concept. */
function expandTags(selected: string[], allTags: TagCount[]): string[] | undefined {
  if (selected.length === 0) return undefined;
  const expanded = selected.flatMap(
    (name) => allTags.find((t) => t.name === name)?.raw_names ?? [name],
  );
  return [...new Set(expanded)];
}

/** Build the current text+tag+collection query for one page of results. */
function buildQuery(s: LibraryState, p: number): SearchQuery {
  const text = s.query.trim();
  const q: SearchQuery = {
    text: text || undefined,
    tags_any: expandTags(s.selectedTags, s.allTags),
    collections: s.collectionFilter ? [s.collectionFilter] : undefined,
    sort_by: 'date',
    sort_order: 'desc',
    page: p,
    page_size: PAGE_SIZE,
  };
  return q;
}

interface LibraryState {
  books: Book[]; isLoading: boolean; isLoadingMore: boolean; error: string | null;
  query: string; selectedTags: string[]; collectionFilter: string | null;
  allTags: TagCount[]; initialized: boolean; total: number; page: number; hasMore: boolean;
  ensureLoaded: () => Promise<void>; refresh: () => Promise<void>; applySearch: () => Promise<void>;
  reload: () => Promise<void>; loadMore: () => Promise<void>; loadTags: (text?: string) => Promise<void>;
  toggleTag: (name: string) => void; importBook: (filePath: string) => Promise<void>; deleteBook: (id: string) => Promise<void>;
}

export const useLibraryStore = create<LibraryState>((set, get) => ({
  books: [], isLoading: false, isLoadingMore: false, error: null,
  query: '', selectedTags: [], collectionFilter: null, allTags: [], initialized: false, total: 0, page: 0, hasMore: false,

  ensureLoaded: async () => {
    // Set the flag BEFORE loading so re-entrant calls during the initial
    // fetch are no-ops (search / filter state is preserved across switches).
    if (get().initialized) return;
    set({ initialized: true });
    await get().refresh();
  },
  refresh: async () => { await Promise.all([get().reload(), get().loadTags()]); },
  applySearch: async () => {
    const text = get().query.trim();
    await get().loadTags(text || undefined);
    // Drop any selection whose tag vanished from the text-filtered results.
    const present = new Set(get().allTags.map((t) => t.name));
    const { selectedTags } = get();
    if (selectedTags.some((n) => !present.has(n))) {
      set({ selectedTags: selectedTags.filter((n) => present.has(n)) });
    }
    await get().reload();
  },
  loadTags: async (text) => {
    try {
      const tags = await api.getAllTags(text, get().collectionFilter ?? undefined);
      set({ allTags: tags });
    } catch { /* keep the previous list on error */ }
  },

  reload: async () => {
    set({ isLoading: true, error: null });
    try {
      const result = await api.searchBooks(buildQuery(get(), 1));
      set({ books: result.books, total: result.total, page: 1, hasMore: result.books.length < result.total, isLoading: false });
    } catch (e) { set({ error: String(e), isLoading: false }); }
  },

  loadMore: async () => {
    const { hasMore, isLoading, isLoadingMore, page } = get();
    if (!hasMore || isLoading || isLoadingMore) return;
    set({ isLoadingMore: true });
    try {
      const result = await api.searchBooks(buildQuery(get(), page + 1));
      set((s) => { const newBooks = [...s.books, ...result.books]; return { books: newBooks, page: s.page + 1, hasMore: newBooks.length < result.total, isLoadingMore: false }; });
    } catch (e) { set({ error: String(e), isLoadingMore: false }); }
  },

  toggleTag: (name) => { set((s) => { const idx = s.selectedTags.indexOf(name); return { selectedTags: idx >= 0 ? s.selectedTags.filter((_, i) => i !== idx) : [...s.selectedTags, name] }; }); void get().reload(); },
  importBook: async (filePath) => { await api.importBook(filePath); await get().applySearch(); void useSettingsStore.getState().syncIfEnabled(); },
  deleteBook: async (id) => { await api.deleteBook(id); await get().applySearch(); },
}));