import { defineStore } from 'pinia';
import { reactive, ref, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { getThumb, setThumb } from '@/services/thumb-cache';
import type { PixivWork, PixivBrowseStatus } from '@/types';
import type { TaskItem } from '@/stores/tasks';

export type PixivTab = 'recommend' | 'following' | 'bookmark';
export type FeedKey = PixivTab | 'search';

/** Per-tab lazy-load state. `offset`/`total` drive 收藏 (paginated by offset);
 *  `page` drives 推荐/关注/搜索 (1-based feed pages). Lives in the store so
 *  the browse state survives view switches until the app quits. */
interface FeedState {
  items: PixivWork[];
  loading: boolean;
  end: boolean;
  offset: number;
  total: number;
  page: number;
}

const PAGE_SIZE = 30;

function emptyFeed(): FeedState {
  return { items: [], loading: false, end: false, offset: 0, total: 0, page: 1 };
}

export const usePixivBrowseStore = defineStore('pixiv-browse', () => {
  const recommend = reactive<FeedState>(emptyFeed());
  const following = reactive<FeedState>(emptyFeed());
  const bookmark = reactive<FeedState>(emptyFeed());
  const search = reactive<FeedState>(emptyFeed());
  const searchKeyword = ref('');
  const coverMap = reactive<Record<string, string | null>>({});
  const statusMap = reactive<Record<string, PixivBrowseStatus>>({});
  const coverLoading = new Set<string>();

  function feedOf(target: FeedKey) {
    if (target === 'search') return search;
    return target === 'recommend'
      ? recommend
      : target === 'following'
        ? following
        : bookmark;
  }

  async function refreshStatus(ids: string[]) {
    if (!ids.length) return;
    try {
      const list = await api.pixivBrowseStatus(ids);
      for (const s of list) statusMap[s.workId] = s;
    } catch (e) {
      console.error('refresh browse status:', e);
    }
  }

  async function loadMore(target: FeedKey) {
    const feed = feedOf(target);
    if (feed.loading || feed.end) return;
    feed.loading = true;
    try {
      let items: PixivWork[] = [];
      if (target === 'recommend') {
        items = await api.listPixivRecommended(feed.page);
      } else if (target === 'following') {
        items = await api.listPixivFollowingFeed(feed.page);
      } else if (target === 'bookmark') {
        const pageRes = await api.listPixivBookmarks(feed.offset, PAGE_SIZE);
        items = pageRes.items;
        feed.total = pageRes.total;
      } else {
        const kw = searchKeyword.value.trim();
        if (!kw) return;
        items = await api.searchPixivIllusts(kw, feed.page);
      }
      if (items.length === 0) {
        feed.end = true;
      } else {
        // Resolve local state first so cards render correctly (no flash).
        await refreshStatus(items.map((w) => w.id));
        feed.items.push(...items);
        feed.page += 1;
        if (target === 'recommend') {
          // top/illust returns the whole landing batch at once — no more pages.
          feed.end = true;
        } else if (target === 'bookmark') {
          feed.offset += items.length;
          if (feed.offset >= feed.total) feed.end = true;
        } else if (items.length < 30) {
          // A short page means we hit the tail.
          feed.end = true;
        }
      }
    } catch (e) {
      console.error('load feed:', target, e);
      if (feed.items.length === 0) feed.end = true;
    } finally {
      feed.loading = false;
    }
  }

  /** Fetch a cover via the backend proxy (i.pximg.net needs a Referer header
   *  the browser can't set on <img>). Cached in coverMap as a blob URL. */
  async function loadCover(w: PixivWork) {
    if (!w.coverUrl || w.id in coverMap || coverLoading.has(w.id)) return;
    coverLoading.add(w.id);
    coverMap[w.id] = null;
    try {
      // IndexedDB first (persists across reloads/view-switches so repeat covers
      // load instantly); miss → fetch via the Pixiv proxy + cache.
      let blob = await getThumb(w.id);
      if (!blob) {
        const bytes = await api.pixivProxyImage(w.coverUrl);
        blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
        void setThumb(w.id, blob);
      }
      coverMap[w.id] = URL.createObjectURL(blob);
    } catch {
      coverMap[w.id] = null;
    } finally {
      coverLoading.delete(w.id);
    }
  }

  function setStatus(workId: string, status: PixivBrowseStatus) {
    statusMap[workId] = status;
  }

  /** Patch the work bound to a task id; returns that workId (or null). */
  function updateByTaskId(
    taskId: string,
    patch: Partial<PixivBrowseStatus>,
  ): string | null {
    for (const [wid, st] of Object.entries(statusMap)) {
      if (st.taskId === taskId) {
        statusMap[wid] = { ...st, ...patch };
        return wid;
      }
    }
    return null;
  }

  /** Drop one feed's items/covers/status and fetch fresh (the manual reload). */
  async function reload(target: FeedKey) {
    const feed = feedOf(target);
    for (const w of feed.items) {
      delete statusMap[w.id];
    }
    // coverMap (blob URLs) is intentionally KEPT — the IndexedDB-backed cover
    // cache + reused blob URLs keep covers stable across reloads instead of
    // the flicker of re-fetching / re-creating object URLs every time.
    Object.assign(feed, emptyFeed());
    await loadMore(target);
  }

  /** Set the current search keyword, reset the search feed, and trigger a new
   *  search. Passing an empty string clears the search. */
  function setSearchKeyword(kw: string) {
    const trimmed = kw.trim();
    searchKeyword.value = trimmed;
    for (const w of search.items) {
      delete statusMap[w.id];
    }
    Object.assign(search, emptyFeed());
    if (trimmed) {
      void loadMore('search');
    }
  }

  /** Log-out: drop every feed's items + browse status (covers stay cached in
   *  IndexedDB so repeat loads are still instant). The view flips to the
   *  logged-out prompt; the next login repopulates from scratch. */
  function resetAll() {
    for (const f of [recommend, following, bookmark, search]) {
      for (const w of f.items) delete statusMap[w.id];
      Object.assign(f, emptyFeed());
    }
  }

  // Auto-load covers whenever any feed gains items.
  watch(() => recommend.items.length, () => recommend.items.forEach(loadCover));
  watch(() => following.items.length, () => following.items.forEach(loadCover));
  watch(() => bookmark.items.length, () => bookmark.items.forEach(loadCover));
  watch(() => search.items.length, () => search.items.forEach(loadCover));

  // Track task progress at the store level (not the component) so card state
  // keeps updating even while the Pixiv view is unmounted — a download that
  // finishes while the user is elsewhere still flips the card to "downloaded".
  // On a terminal status, re-resolve the work (completed → local book appears).
  const TERMINAL = ['completed', 'failed', 'cancelled'];
  let unlistenProgress: UnlistenFn | undefined;
  void listen<TaskItem>('task://progress', (event) => {
    const p = event.payload;
    const wid = updateByTaskId(p.id, {
      taskStatus: p.status,
      progressCurrent: p.progress_current,
      progressTotal: p.progress_total,
    });
    if (wid && TERMINAL.includes(p.status)) refreshStatus([wid]);
  }).then((fn) => {
    unlistenProgress = fn;
  });

  return {
    recommend,
    following,
    bookmark,
    search,
    searchKeyword,
    coverMap,
    statusMap,
    loadMore,
    reload,
    refreshStatus,
    setStatus,
    updateByTaskId,
    setSearchKeyword,
    resetAll,
  };
});
