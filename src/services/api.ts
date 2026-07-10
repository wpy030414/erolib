import { invoke } from '@tauri-apps/api/core';
import { open as dialogOpen } from '@tauri-apps/plugin-dialog';
import type {
  Book,
  BookMetadata,
  Collection,
  SearchQuery,
  SearchResult,
  Tag,
} from '../types';

export const api = {
  // Book operations
  importBook: (filePath: string) =>
    invoke<Book>('import_book', { filePath }),

  importBookFromImages: (images: number[][], metadata: BookMetadata) =>
    invoke<Book>('import_book_from_images', { images, metadata }),

  deleteBook: (id: string) =>
    invoke<void>('delete_book', { id }),

  updateBookMetadata: (id: string, metadata: BookMetadata) =>
    invoke<Book>('update_book_metadata', { id, metadata }),

  getBookCover: (id: string) =>
    invoke<number[]>('get_book_cover', { id }),

  exportBook: (id: string, format: string) =>
    invoke<string>('export_book', { id, format }),

  // Copy a book file to a user-chosen location (right-click → 保存到本地).
  saveBook: (id: string, dest: string) =>
    invoke<void>('save_book', { id, dest }),

  listBooks: (limit?: number, offset?: number) =>
    invoke<Book[]>('list_books', { limit, offset }),

  getBook: (id: string) =>
    invoke<Book>('get_book', { id }),

  getBookPageCount: (id: string) =>
    invoke<number>('get_book_page_count', { id }),

  getBookPage: (id: string, page: number) =>
    invoke<number[]>('get_book_page', { id, page }),

  // Search
  searchBooks: (query: SearchQuery) =>
    invoke<SearchResult>('search_books', { query }),

  getAllTags: () => invoke<Tag[]>('get_all_tags'),

  getAllCollections: () => invoke<Collection[]>('get_all_collections'),

  // OPDS Server (kept; lives under Settings only).
  startOpdsServer: (port: number) =>
    invoke<string>('start_opds_server', { port }),

  stopOpdsServer: () => invoke<void>('stop_opds_server'),

  // RSS Server (kept; lives under Settings only).
  startRssServer: (port: number) =>
    invoke<string>('start_rss_server', { port }),

  stopRssServer: () => invoke<void>('stop_rss_server'),

  // Pixiv bookmarks
  testPixivCookie: (cookie: string) =>
    invoke<{ ok: boolean; has_phpsessid: boolean; cookie_length: number }>(
      'pixiv_test_cookie',
      { cookie },
    ),

  downloadPixivBookmarks: (cookie: string, userId: string, limit: number) =>
    invoke<void>('pixiv_download_bookmarks', { cookie, userId, limit }),

  cancelPixivDownload: () => invoke<void>('pixiv_cancel_download'),

  // EHentai in-app login
  openEHentaiLoginWindow: () =>
    invoke<void>('ehentai_open_login_window'),

  getEHentaiLogin: () =>
    invoke<string | null>('ehentai_get_login'),

  downloadEHentaiGallery: (galleryUrl: string) =>
    invoke<void>('ehentai_download_gallery', { galleryUrl }),

  cancelEHentaiDownload: () => invoke<void>('ehentai_cancel_download'),

  // Pixiv in-app login
  getPixivLogin: () =>
    invoke<{ cookie: string; user_id: string } | null>('pixiv_get_login'),

  setPixivLogin: (cookie: string, userId: string) =>
    invoke<void>('pixiv_set_login', { cookie, userId }),

  openPixivLoginWindow: () => invoke<void>('pixiv_open_login_window'),

  reLoginPixiv: () => invoke<void>('pixiv_clear_login'),

  // Pixiv followings + per-user works
  fetchPixivFollowings: (limit: number) =>
    invoke<Array<{ userId: string; userName: string; profileImageUrl: string }>>(
      'pixiv_fetch_followings',
      { limit },
    ),

  downloadPixivUserWorks: (targetUserId: string, limit: number) =>
    invoke<void>('pixiv_download_user_works', {
      targetUserId,
      limit,
    }),

  // Dialog
  openFile: (filters?: Array<{ name: string; extensions: string[] }>) =>
    dialogOpen({
      multiple: false,
      filters: filters ?? [
        { name: 'Comic', extensions: ['cb7', 'cbz', 'cbr', 'pdf'] },
      ],
    }),
};
