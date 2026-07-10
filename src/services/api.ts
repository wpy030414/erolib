import { invoke } from '@tauri-apps/api/core';
import { open as dialogOpen } from '@tauri-apps/plugin-dialog';
import type {
  Book,
  BookMetadata,
  Collection,
  PixivBrowseStatus,
  PixivWork,
  SearchQuery,
  SearchResult,
  TagCount,
} from '../types';

export interface TaskItem {
  id: string;
  source: string;
  status: string;
  title: string;
  detail: string;
  progress_current: number;
  progress_total: number;
  retry_count: number;
  max_retries: number;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

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
  /** Low-res JPEG thumbnail (≤256px) — small over IPC, cached in IndexedDB. */
  getBookCoverThumb: (id: string) =>
    invoke<number[]>('get_book_cover_thumb', { id }),

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

  getAllTags: (text?: string) => invoke<TagCount[]>('get_all_tags', { text }),

  getAllCollections: () => invoke<Collection[]>('get_all_collections'),

  // OPDS Server (kept; lives under Settings Sharing tab).
  startOpdsServer: (port: number) =>
    invoke<string>('start_opds_server_cmd', { port }),

  stopOpdsServer: () => invoke<void>('stop_opds_server_cmd'),

  // RSS Server (kept; lives under Settings Sharing tab).
  startRssServer: (port: number) =>
    invoke<string>('start_rss_server_cmd', { port }),

  stopRssServer: () => invoke<void>('stop_rss_server_cmd'),

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

  setEHentaiLogin: (cookie: string) =>
    invoke<void>('ehentai_set_login', { cookie }),

  downloadEHentaiGallery: (galleryUrl: string) =>
    invoke<void>('ehentai_download_gallery', { galleryUrl }),

  cancelEHentaiDownload: () => invoke<void>('ehentai_cancel_download'),

  // Pixiv in-app login
  getPixivLogin: () =>
    invoke<{ cookie: string; user_id: string; user_name?: string } | null>('pixiv_get_login'),

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

  // Pixiv browse grid (关注/收藏 tabs)
  listPixivBookmarks: (offset: number, limit: number) =>
    invoke<{ items: PixivWork[]; total: number }>('pixiv_list_bookmarks', { offset, limit }),

  listPixivFollowingFeed: (page: number) =>
    invoke<PixivWork[]>('pixiv_list_following_feed', { page }),

  pixivProxyImage: (url: string) => invoke<number[]>('pixiv_proxy_image', { url }),

  // Pixiv browse card state (local book / active task) for a batch of work ids
  pixivBrowseStatus: (workIds: string[]) =>
    invoke<PixivBrowseStatus[]>('pixiv_browse_status', { workIds }),

  // Reset
  resetAppData: () => invoke<void>('reset_app_data'),

  // Tasks
  tasksList: () => invoke<TaskItem[]>('tasks_list'),

  taskPause: (taskId: string) =>
    invoke<void>('task_pause', { taskId }),

  taskResume: (taskId: string) =>
    invoke<void>('task_resume', { taskId }),

  taskCancel: (taskId: string) =>
    invoke<void>('task_cancel', { taskId }),

  taskDelete: (taskId: string) =>
    invoke<void>('task_delete', { taskId }),

  taskRetry: (taskId: string) =>
    invoke<void>('task_retry', { taskId }),

  taskEnqueuePixivBookmarks: (cookie: string, userId: string, limit: number) =>
    invoke<string>('task_enqueue_pixiv_bookmarks', { cookie, userId, limit }),

  taskEnqueuePixivUserWorks: (cookie: string, targetUserId: string, limit: number) =>
    invoke<string>('task_enqueue_pixiv_user_works', { cookie, targetUserId, limit }),

  taskEnqueueEhentaiGallery: (cookie: string, galleryUrl: string) =>
    invoke<string>('task_enqueue_ehentai_gallery', { cookie, galleryUrl }),

  taskEnqueuePixivWork: (cookie: string, workId: string, title: string) =>
    invoke<string>('task_enqueue_pixiv_work', { cookie, workId, title }),

  openFile: (filters?: Array<{ name: string; extensions: string[] }>) =>
    dialogOpen({
      multiple: false,
      filters: filters ?? [
        { name: 'Comic', extensions: ['cb7', 'cbz', 'cbr', 'pdf'] },
      ],
    }),
};
