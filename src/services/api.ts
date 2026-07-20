import { invoke } from '@tauri-apps/api/core';
import { open as dialogOpen } from '@tauri-apps/plugin-dialog';
import type {
  AhentaiBrowseStatus,
  AhentaiGalleryItem,
  Book,
  Collection,
  EhentaiBrowseStatus,
  GalleryListItem,
  NicecatBrowseStatus,
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
  speed: number;
  total_bytes: number;
  elapsed_ms: number;
  logs: string[];
  book_id: string | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

export const api = {
  // Book operations
  importBook: (filePath: string) =>
    invoke<Book>('import_book', { filePath }),

  deleteBook: (id: string) =>
    invoke<void>('delete_book', { id }),

  /** Low-res JPEG thumbnail (≤256px) — small over IPC, cached in IndexedDB. */
  getBookCoverThumb: (id: string) =>
    invoke<number[]>('get_book_cover_thumb', { id }),

  // Copy a book file to a user-chosen location (right-click → 保存到本地).
  saveBook: (id: string, dest: string) =>
    invoke<void>('save_book', { id, dest }),

  // Export a single page image to a user-chosen location (reader right-click).
  saveBookPage: (id: string, page: number, dest: string) =>
    invoke<void>('save_book_page', { id, page, dest }),

  listBooks: (limit?: number, offset?: number) =>
    invoke<Book[]>('list_books', { limit, offset }),

  // Reading-time tracking — one session_id is minted per Reader mount by
  // `open_book`; `record_reading` is fire-and-forget and the backend stores
  // the latest per-session delta (last-write-wins on `duration_ms`).
  openBook: (id: string) =>
    invoke<number>('open_book', { id }),

  recordReading: (id: string, sessionId: number, durationMs: number) =>
    invoke<void>('record_reading', { id, sessionId, durationMs }),

  // Home-page aggregates.
  getWeeklyReadingMs: () =>
    invoke<number>('get_weekly_reading_ms'),

  listRecentBooks: (limit: number) =>
    invoke<Book[]>('list_recent_books', { limit }),

  // One-way local sync: mirror the library into a directory as
  // ${title}-${metaHash}.cb7 (copies new books, mirror-deletes removed).
  syncToDir: (targetDir: string) =>
    invoke<{ copied: number; skipped: number }>('sync_to_dir', { targetDir }),

  getBook: (id: string) =>
    invoke<Book>('get_book', { id }),

  getBookPageCount: (id: string) =>
    invoke<number>('get_book_page_count', { id }),

  getBookPage: (id: string, page: number) =>
    invoke<ArrayBuffer>('get_book_page', { id, page }),

  // Search
  searchBooks: (query: SearchQuery) =>
    invoke<SearchResult>('search_books', { query }),

  getAllTags: (text?: string, collection?: string) =>
    invoke<TagCount[]>('get_all_tags', { text, collection }),

  // Persist the app locale so SQL renders tags in the current language.
  setLocale: (locale: string) =>
    invoke<void>('set_locale', { localeStr: locale }),

  // OPDS Server (kept; lives under Settings Sharing tab).
  startOpdsServer: (port: number) =>
    invoke<string>('start_opds_server_cmd', { port }),

  stopOpdsServer: () => invoke<void>('stop_opds_server_cmd'),

  // RSS Server (kept; lives under Settings Sharing tab).
  startRssServer: (port: number) =>
    invoke<string>('start_rss_server_cmd', { port }),

  stopRssServer: () => invoke<void>('stop_rss_server_cmd'),

  // EHentai in-app login
  openEHentaiLoginWindow: () =>
    invoke<void>('ehentai_open_login_window'),

  getEHentaiLogin: () =>
    invoke<string | null>('ehentai_get_login'),

  // EHentai browse grid (search + proxied thumbs + per-gallery state)
  ehentaiSearch: (keyword: string | null, category: string | null, next: string | null, ex: boolean) =>
    invoke<GalleryListItem[]>('ehentai_search', { keyword, category, next, ex }),

  ehentaiProxyThumb: (url: string) =>
    invoke<number[]>('ehentai_proxy_thumb', { url }),

  ehentaiBrowseStatus: (galleryUrls: string[]) =>
    invoke<EhentaiBrowseStatus[]>('ehentai_browse_status', { galleryUrls }),

  // AHentai browse grid (no login — search + proxied thumbs + per-gallery state)
  ahentaiSearch: (keyword: string | null, page: number | null) =>
    invoke<AhentaiGalleryItem[]>('ahentai_search', { keyword, page }),

  ahentaiProxyThumb: (url: string) =>
    invoke<number[]>('ahentai_proxy_thumb', { url }),

  ahentaiBrowseStatus: (galleryIds: string[]) =>
    invoke<AhentaiBrowseStatus[]>('ahentai_browse_status', { galleryIds }),

  // NiceCat browse — pure HTTP via RC4 token auth (no WebView needed).
  nicecatFetchApi: (path: string, formFields: Record<string, string>) =>
    invoke<any>('nicecat_fetch_api', { path, formFields }),

  nicecatProxyThumb: (url: string) =>
    invoke<number[]>('nicecat_proxy_thumb', { url }),

  nicecatBrowseStatus: (comicIds: string[]) =>
    invoke<NicecatBrowseStatus[]>('nicecat_browse_status', { comicIds }),

  // Pixiv in-app login
  getPixivLogin: () =>
    invoke<{ cookie: string; user_id: string; user_name?: string } | null>('pixiv_get_login'),

  setPixivLogin: (cookie: string, userId: string) =>
    invoke<void>('pixiv_set_login', { cookie, userId }),

  openPixivLoginWindow: () => invoke<void>('pixiv_open_login_window'),

  pixivLogout: () => invoke<void>('pixiv_clear_login'),

  ehentaiLogout: () => invoke<void>('ehentai_clear_login'),

  // Pixiv browse grid (关注/收藏 tabs)
  listPixivBookmarks: (offset: number, limit: number) =>
    invoke<{ items: PixivWork[]; total: number }>('pixiv_list_bookmarks', { offset, limit }),

  listPixivFollowingFeed: (page: number) =>
    invoke<PixivWork[]>('pixiv_list_following_feed', { page }),

  listPixivRecommended: (page: number) =>
    invoke<PixivWork[]>('pixiv_list_recommended', { page }),

  searchPixivIllusts: (keyword: string, page: number) =>
    invoke<PixivWork[]>('pixiv_search_illusts', { keyword, page }),

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

  tasksClearCompleted: () =>
    invoke<number>('tasks_clear_completed'),

  tasksRetryAll: () =>
    invoke<[number, number]>('tasks_retry_all'),

  taskEnqueueEhentaiGallery: (cookie: string, galleryUrl: string, title: string) =>
    invoke<string>('task_enqueue_ehentai_gallery', { cookie, galleryUrl, title }),

  taskEnqueuePixivWork: (cookie: string, workId: string, title: string) =>
    invoke<string>('task_enqueue_pixiv_work', { cookie, workId, title }),

  taskEnqueueAhentaiGallery: (galleryId: string, title: string) =>
    invoke<string>('task_enqueue_ahentai_gallery', { galleryId, title }),

  taskEnqueueNicecatGallery: (comicId: string, title: string) =>
    invoke<string>('task_enqueue_nicecat_gallery', { comicId, title }),

  openFile: (filters?: Array<{ name: string; extensions: string[] }>) =>
    dialogOpen({
      multiple: false,
      filters: filters ?? [
        { name: 'Comic', extensions: ['cb7', 'cbz', 'cbr', 'pdf'] },
      ],
    }),

  // Collections (reading lists)
  listCollections: () =>
    invoke<Collection[]>('list_collections'),

  reorderCollections: (positions: [string, number][]) =>
    invoke<void>('reorder_collections', { positions }),

  createCollection: (name: string) =>
    invoke<Collection>('create_collection', { name }),

  renameCollection: (id: string, name: string) =>
    invoke<void>('rename_collection', { id, name }),

  deleteCollection: (id: string) =>
    invoke<void>('delete_collection', { id }),

  addBookToCollection: (collectionId: string, bookId: string) =>
    invoke<void>('add_book_to_collection', { collectionId, bookId }),

  removeBookFromCollection: (collectionId: string, bookId: string) =>
    invoke<void>('remove_book_from_collection', { collectionId, bookId }),

  getBookCollections: (bookId: string) =>
    invoke<string[]>('get_book_collections', { bookId }),

  // App self-update
  checkUpdate: () => invoke<UpdateInfo>('check_update'),

  downloadUpdate: (url: string, name: string) =>
    invoke<string>('download_update', { url, name }),

  installUpdate: (path: string) =>
    invoke<void>('install_update', { path }),

  quitAndInstall: (path: string) =>
    invoke<void>('quit_and_install', { path }),
};

export interface UpdateAsset {
  name: string;
  url: string;
  size: number;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  hasUpdate: boolean;
  notes: string;
  asset: UpdateAsset | null;
}

export interface UpdateProgress {
  percent: number;
  speed: number;
  completed: number;
  total: number;
}
