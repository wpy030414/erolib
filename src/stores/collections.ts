import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { api } from '@/services/api';
import type { Collection } from '@/types';

export const useCollectionsStore = defineStore('collections', () => {
  const collections = ref<Collection[]>([]);
  /** null = "All" (the default, non-deletable pseudo-collection). Does NOT
   *  persist across app restarts — always starts at "All". */
  const activeCollectionId = ref<string | null>(null);
  const initialized = ref(false);

  const activeCollectionName = computed(() => {
    if (activeCollectionId.value === null) return '';
    return collections.value.find((c) => c.id === activeCollectionId.value)?.name ?? '';
  });

  const isAllActive = computed(() => activeCollectionId.value === null);

  async function ensureLoaded() {
    if (initialized.value) return;
    initialized.value = true;
    await fetchCollections();
  }

  async function fetchCollections() {
    try {
      collections.value = await api.listCollections();
    } catch {
      // keep stale list on error
    }
  }

  /** Re-fetch collections from the backend (e.g. after a book is added). */
  async function refresh() {
    await fetchCollections();
  }

  async function reorder(positions: [string, number][]): Promise<boolean> {
    try {
      await api.reorderCollections(positions);
      return true;
    } catch {
      return false;
    }
  }

  async function createCollection(name: string): Promise<Collection | null> {
    try {
      const c = await api.createCollection(name.trim());
      collections.value.push(c);
      return c;
    } catch {
      return null;
    }
  }

  async function renameCollection(id: string, name: string): Promise<boolean> {
    try {
      await api.renameCollection(id, name.trim());
      const c = collections.value.find((x) => x.id === id);
      if (c) c.name = name.trim();
      return true;
    } catch {
      return false;
    }
  }

  async function deleteCollection(id: string): Promise<boolean> {
    try {
      await api.deleteCollection(id);
      collections.value = collections.value.filter((c) => c.id !== id);
      if (activeCollectionId.value === id) {
        activeCollectionId.value = null;
      }
      return true;
    } catch {
      return false;
    }
  }

  async function getBookCollections(bookId: string): Promise<string[]> {
    try {
      return await api.getBookCollections(bookId);
    } catch {
      return [];
    }
  }

  async function addBookToCollection(collectionId: string, bookId: string): Promise<boolean> {
    try {
      await api.addBookToCollection(collectionId, bookId);
      return true;
    } catch {
      return false;
    }
  }

  async function removeBookFromCollection(collectionId: string, bookId: string): Promise<boolean> {
    try {
      await api.removeBookFromCollection(collectionId, bookId);
      return true;
    } catch {
      return false;
    }
  }

  function setActiveCollection(id: string | null) {
    activeCollectionId.value = id;
  }

  return {
    collections,
    activeCollectionId,
    activeCollectionName,
    isAllActive,
    initialized,
    ensureLoaded,
    fetchCollections,
    refresh,
    reorder,
    createCollection,
    renameCollection,
    deleteCollection,
    getBookCollections,
    addBookToCollection,
    removeBookFromCollection,
    setActiveCollection,
  };
});
