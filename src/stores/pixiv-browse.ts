import { defineStore } from 'pinia';
import { reactive, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import type { PixivWork, PixivBrowseStatus } from '@/types';
import type { TaskItem } from '@/stores/tasks';

export type PixivTab = 'following' | 'bookmark';

/** Per-tab lazy-load state. `offset`/`total` drive 收藏 (paginated by offset);
 *  `page` drives 关注 (1-based feed pages). Lives in the store so the browse
 *  state survives view switches until the app quits. */
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
  const following = reactive<FeedState>(emptyFeed());
  const bookmark = reactive<FeedState>(emptyFeed());
  const coverMap = reactive<Record<string, string | null>>({});
  const statusMap = reactive<Record<string, PixivBrowseStatus>>({});
  const coverLoading = new Set<string>();

  function feedOf(tab: PixivTab) {
    return tab === 'following' ? following : bookmark;
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

  async function loadMore(tab: PixivTab) {
    const feed = feedOf(tab);
    if (feed.loading || feed.end) return;
    feed.loading = true;
    try {
      if (tab === 'following') {
        const items = await api.listPixivFollowingFeed(feed.page);
        if (items.length === 0) {
          feed.end = true;
        } else {
          // Resolve local state first so cards render correctly (no flash).
          await refreshStatus(items.map((w) => w.id));
          feed.items.push(...items);
          feed.page += 1;
          // follow_latest returns ~60 per page; a short page means we hit the tail.
          if (items.length < 30) feed.end = true;
        }
      } else {
        const { items, total } = await api.listPixivBookmarks(feed.offset, PAGE_SIZE);
        feed.total = total;
        await refreshStatus(items.map((w) => w.id));
        feed.items.push(...items);
        feed.offset += items.length;
        if (items.length === 0 || feed.offset >= total) feed.end = true;
      }
    } catch (e) {
      console.error('load feed:', tab, e);
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
      const bytes = await api.pixivProxyImage(w.coverUrl);
      const blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
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

  /** Drop one tab's items/covers/status and fetch fresh (the manual reload). */
  async function reload(tab: PixivTab) {
    const feed = feedOf(tab);
    for (const w of feed.items) {
      const url = coverMap[w.id];
      if (url) URL.revokeObjectURL(url);
      delete coverMap[w.id];
      delete statusMap[w.id];
    }
    Object.assign(feed, emptyFeed());
    await loadMore(tab);
  }

  // Auto-load covers whenever either feed gains items.
  watch(() => following.items.length, () => following.items.forEach(loadCover));
  watch(() => bookmark.items.length, () => bookmark.items.forEach(loadCover));

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
    following,
    bookmark,
    coverMap,
    statusMap,
    loadMore,
    reload,
    refreshStatus,
    setStatus,
    updateByTaskId,
  };
});
