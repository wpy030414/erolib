import { create } from 'zustand';
import { api } from '@/services/api';
import type { Collection } from '@/types';

interface CollectionsState {
  collections: Collection[]; activeCollectionId: string | null; initialized: boolean;
  readonly activeCollectionName: string; readonly isAllActive: boolean;
  ensureLoaded: () => Promise<void>; fetchCollections: () => Promise<void>; refresh: () => Promise<void>;
  reorder: (positions: [string, number][]) => Promise<boolean>;
  createCollection: (name: string) => Promise<Collection | null>;
  renameCollection: (id: string, name: string) => Promise<boolean>;
  deleteCollection: (id: string) => Promise<boolean>;
  getBookCollections: (bookId: string) => Promise<string[]>;
  addBookToCollection: (cId: string, bId: string) => Promise<boolean>;
  removeBookFromCollection: (cId: string, bId: string) => Promise<boolean>;
  setActiveCollection: (id: string | null) => void;
}

export const useCollectionsStore = create<CollectionsState>((set, get) => ({
  collections: [], activeCollectionId: null, initialized: false,
  // Derived like the Vue computed versions: rename/delete/update flows can't
  // drift from `collections` + `activeCollectionId`.
  get activeCollectionName() { return get().collections.find((c) => c.id === get().activeCollectionId)?.name ?? ''; },
  get isAllActive() { return get().activeCollectionId === null; },

  // Set the flag BEFORE loading so re-entrant calls during the initial fetch
  // are no-ops.
  ensureLoaded: async () => { if (get().initialized) return; set({ initialized: true }); await get().fetchCollections(); },
  fetchCollections: async () => { try { const cols = await api.listCollections(); set({ collections: cols }); } catch { /* ignore */ } },
  refresh: async () => { await get().fetchCollections(); },
  reorder: async (positions) => { try { await api.reorderCollections(positions); return true; } catch { return false; } },

  createCollection: async (name) => {
    try { const col = await api.createCollection(name.trim()); set((s) => ({ collections: [...s.collections, col] })); return col; } catch { return null; }
  },
  renameCollection: async (id, name) => {
    try { await api.renameCollection(id, name.trim()); set((s) => ({ collections: s.collections.map((c) => c.id === id ? { ...c, name: name.trim() } : c) })); return true; } catch { return false; }
  },
  deleteCollection: async (id) => {
    try {
      await api.deleteCollection(id);
      // Deleting the active collection falls back to "All".
      set((s) => ({ collections: s.collections.filter((c) => c.id !== id), activeCollectionId: s.activeCollectionId === id ? null : s.activeCollectionId }));
      return true;
    } catch { return false; }
  },
  getBookCollections: async (bookId) => { try { return await api.getBookCollections(bookId); } catch { return []; } },
  addBookToCollection: async (cId, bId) => { try { await api.addBookToCollection(cId, bId); return true; } catch { return false; } },
  removeBookFromCollection: async (cId, bId) => { try { await api.removeBookFromCollection(cId, bId); return true; } catch { return false; } },
  setActiveCollection: (id) => set({ activeCollectionId: id }),
}));
