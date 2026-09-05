import { create } from 'zustand';
import { api } from '@/services/api';
import type { Collection } from '@/types';

interface CollectionsState {
  collections: Collection[]; activeCollectionId: string | null; initialized: boolean;
  activeCollectionName: string; isAllActive: boolean;
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
  collections: [], activeCollectionId: null, initialized: false, activeCollectionName: '', isAllActive: true,

  ensureLoaded: async () => { if (get().initialized) return; await get().fetchCollections(); set({ initialized: true }); },
  fetchCollections: async () => { try { const cols = await api.listCollections(); set({ collections: cols }); } catch { /* ignore */ } },
  refresh: async () => { await get().fetchCollections(); },
  reorder: async (positions) => { try { await api.reorderCollections(positions); return true; } catch { return false; } },

  createCollection: async (name) => {
    try { const col = await api.createCollection(name); set((s) => ({ collections: [...s.collections, col] })); return col; } catch { return null; }
  },
  renameCollection: async (id, name) => {
    try { await api.renameCollection(id, name); set((s) => ({ collections: s.collections.map((c) => c.id === id ? { ...c, name } : c) })); return true; } catch { return false; }
  },
  deleteCollection: async (id) => {
    try {
      await api.deleteCollection(id);
      set((s) => ({ collections: s.collections.filter((c) => c.id !== id), activeCollectionId: s.activeCollectionId === id ? null : s.activeCollectionId, activeCollectionName: s.activeCollectionId === id ? '' : s.activeCollectionName, isAllActive: s.activeCollectionId === id }));
      return true;
    } catch { return false; }
  },
  getBookCollections: async (bookId) => { try { return await api.getBookCollections(bookId); } catch { return []; } },
  addBookToCollection: async (cId, bId) => { try { await api.addBookToCollection(cId, bId); return true; } catch { return false; } },
  removeBookFromCollection: async (cId, bId) => { try { await api.removeBookFromCollection(cId, bId); return true; } catch { return false; } },
  setActiveCollection: (id) => { const name = get().collections.find((c) => c.id === id)?.name ?? ''; set({ activeCollectionId: id, activeCollectionName: name, isAllActive: id === null }); },
}));