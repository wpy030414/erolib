import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/services/api';
import type { Book, SearchQuery, SearchResult } from '@/types';

export const useLibraryStore = defineStore('library', () => {
  const books = ref<Book[]>([]);
  const isLoading = ref(false);
  const error = ref<string | null>(null);
  const query = ref('');

  async function refresh() {
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

  async function search(text: string) {
    isLoading.value = true;
    error.value = null;
    try {
      const q: SearchQuery = {
        text: text.trim() || undefined,
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

  async function importBook(filePath: string) {
    await api.importBook(filePath);
    await refresh();
  }

  async function deleteBook(id: string) {
    await api.deleteBook(id);
    await refresh();
  }

  return {
    books,
    isLoading,
    error,
    query,
    refresh,
    search,
    importBook,
    deleteBook,
  };
});
