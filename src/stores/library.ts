import { create } from 'zustand';
import { api } from '@/services/api';
import type { Book, TagCount, SearchQuery } from '@/types';

const PAGE_SIZE = 48;

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

  ensureLoaded: async () => { if (get().initialized) return; await get().applySearch(); set({ initialized: true }); },
  refresh: async () => { await Promise.all([get().reload(), get().loadTags()]); },
  applySearch: async () => { await get().loadTags(get().query); const currentTags = get().allTags.map((t) => t.name); const stale = get().selectedTags.filter((t) => !currentTags.includes(t)); if (stale.length > 0) set((s) => ({ selectedTags: s.selectedTags.filter((t) => !stale.includes(t)) })); await get().reload(); },
  loadTags: async (text) => { try { const tags = await api.getAllTags(text ?? '', get().collectionFilter ?? undefined); set({ allTags: tags }); } catch { /* ignore */ } },

  reload: async () => {
    set({ isLoading: true, error: null, page: 0 });
    try {
      const { query, selectedTags, collectionFilter } = get();
      const q: SearchQuery = { text: query || undefined, sort_by: 'date', sort_order: 'desc', page: 1, page_size: PAGE_SIZE };
      if (collectionFilter) q.collections = [collectionFilter];
      if (selectedTags.length > 0) q.tags_any = selectedTags;
      const result = await api.searchBooks(q);
      set({ books: result.books, total: result.total, page: 1, hasMore: result.books.length < result.total, isLoading: false });
    } catch (e) { set({ error: String(e), isLoading: false }); }
  },

  loadMore: async () => {
    const { hasMore, isLoading, isLoadingMore, page } = get();
    if (!hasMore || isLoading || isLoadingMore) return;
    set({ isLoadingMore: true });
    try {
      const { query, selectedTags, collectionFilter } = get();
      const q: SearchQuery = { text: query || undefined, sort_by: 'date', sort_order: 'desc', page: page + 1, page_size: PAGE_SIZE };
      if (collectionFilter) q.collections = [collectionFilter];
      if (selectedTags.length > 0) q.tags_any = selectedTags;
      const result = await api.searchBooks(q);
      set((s) => { const newBooks = [...s.books, ...result.books]; return { books: newBooks, page: s.page + 1, hasMore: newBooks.length < result.total, isLoadingMore: false }; });
    } catch (e) { set({ error: String(e), isLoadingMore: false }); }
  },

  toggleTag: (name) => { set((s) => { const idx = s.selectedTags.indexOf(name); return { selectedTags: idx >= 0 ? s.selectedTags.filter((_, i) => i !== idx) : [...s.selectedTags, name] }; }); void get().reload(); },
  importBook: async (filePath) => { await api.importBook(filePath); await get().applySearch(); },
  deleteBook: async (id) => { await api.deleteBook(id); await get().applySearch(); },
}));