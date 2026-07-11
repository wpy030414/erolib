import { defineStore } from 'pinia';
import { reactive, ref } from 'vue';
import { api } from '@/services/api';
import { useBrowseFeed, type BrowseFeedShared } from '@/composables/useBrowseFeed';
import type { PixivWork, PixivBrowseStatus } from '@/types';

export type PixivTab = 'recommend' | 'following' | 'bookmark';
export type FeedKey = PixivTab | 'search';

// Following & search return a fixed per-page from Pixiv (~30 / ~60); this is
// the end-of-feed heuristic for those (a short page ⇒ last page). The grid's
// actual page size is 48 — see useBrowseFeed's BROWSE_PAGE_SIZE.
const SOURCE_END_HINT = 30;
// Bookmark limit is tunable, so fetch as much as Pixiv allows per request —
// the 48/page buffer then needs fewer round-trips, and any surplus past a page
// boundary stays buffered for the next loadMore. 100 is within Pixiv's range.
const BOOKMARK_FETCH = 100;

/**
 * Pixiv browse store — a thin instantiation layer over `useBrowseFeed`.
 *
 * The four feeds (recommend / following / bookmark / search) each inject their
 * own `fetchPage` (that's where the per-tab pagination model lives: recommend
 * is one-shot, bookmark is offset/total, following & search are page-based),
 * but they SHARE one coverMap / statusMap / coverLoading — the same Pixiv work
 * routinely appears in several tabs, so a cover or download state resolved in
 * one must be visible in the others. Only the recommend instance arms the
 * progress listener for the group. */
export const usePixivBrowseStore = defineStore('pixiv-browse', () => {
  const searchKeyword = ref('');

  // Shared across all four feeds: a work's cover/status is independent of
  // which tab it was first seen in.
  const shared: BrowseFeedShared<PixivBrowseStatus> = {
    coverMap: reactive<Record<string, string | null>>({}),
    statusMap: reactive<Record<string, PixivBrowseStatus>>({}),
    coverLoading: new Set<string>(),
  };

  const common = {
    keyOf: (w: PixivWork) => w.id,
    statusKeyOf: (s: PixivBrowseStatus) => s.workId,
    coverKeyOf: (w: PixivWork) => w.id,
    coverUrlOf: (w: PixivWork) => w.coverUrl ?? null,
    fetchStatus: (ids: string[]) => api.pixivBrowseStatus(ids),
    proxyCover: (url: string) => api.pixivProxyImage(url),
    shared,
  };

  // 推荐 (top/illust): one-shot — the landing batch comes back in full.
  const recommend = useBrowseFeed<PixivWork, string, PixivBrowseStatus, number>({
    ...common,
    listen: true,
    initialCursor: 1,
    fetchPage: async (cursor) => ({
      items: await api.listPixivRecommended(cursor),
      // No further pages regardless of batch size.
      nextCursor: cursor,
      end: true,
    }),
  });

  // 关注 feed: 1-based pages, ~30/页.
  const following = useBrowseFeed<PixivWork, string, PixivBrowseStatus, number>({
    ...common,
    listen: false,
    initialCursor: 1,
    fetchPage: async (cursor) => {
      const items = await api.listPixivFollowingFeed(cursor);
      return { items, nextCursor: cursor + 1, end: items.length < SOURCE_END_HINT };
    },
  });

  // 收藏: offset-paginated with a known total.
  const bookmark = useBrowseFeed<PixivWork, string, PixivBrowseStatus, number>({
    ...common,
    listen: false,
    initialCursor: 0,
    fetchPage: async (cursor) => {
      const pageRes = await api.listPixivBookmarks(cursor, BOOKMARK_FETCH);
      const nextCursor = cursor + pageRes.items.length;
      return { items: pageRes.items, nextCursor, end: nextCursor >= pageRes.total };
    },
  });

  // 搜索: page-based like following, driven by `searchKeyword`.
  const search = useBrowseFeed<PixivWork, string, PixivBrowseStatus, number>({
    ...common,
    listen: false,
    initialCursor: 1,
    fetchPage: async (cursor) => {
      const kw = searchKeyword.value.trim();
      if (!kw) return { items: [], nextCursor: cursor, end: true };
      const items = await api.searchPixivIllusts(kw, cursor);
      return { items, nextCursor: cursor + 1, end: items.length < SOURCE_END_HINT };
    },
  });

  function instOf(target: FeedKey) {
    if (target === 'search') return search;
    if (target === 'recommend') return recommend;
    if (target === 'following') return following;
    return bookmark;
  }

  function loadMore(target: FeedKey) {
    return instOf(target).loadMore();
  }

  function reload(target: FeedKey) {
    return instOf(target).reload();
  }

  /** Optimistically mark a work as downloading (the view does this right after
   *  enqueuing so the mask shows before the first progress tick). Writes to
   *  the shared statusMap, so the state is visible on every tab that shows
   *  this work. */
  function setStatus(workId: string, status: PixivBrowseStatus) {
    return recommend.setStatus(workId, status);
  }

  /** Commit the search box text: reset the search feed and fire the first
   *  page. An empty query clears the search (back to recommend). */
  function setSearchKeyword(kw: string) {
    const trimmed = kw.trim();
    searchKeyword.value = trimmed;
    search.resetFeed();
    if (trimmed) void search.loadMore();
  }

  /** Log-out: drop every feed's items + cursor and clear the shared statusMap
   *  (a different account may have different local state). coverMap is kept —
   *  IndexedDB-backed covers survive across logins. */
  function resetAll() {
    searchKeyword.value = '';
    recommend.resetFeed();
    following.resetFeed();
    bookmark.resetFeed();
    search.resetFeed();
    recommend.clearStatusMap();
  }

  return {
    // Per-tab feed state (bind to <FeedList :feed="...">).
    recommend: recommend.feed,
    following: following.feed,
    bookmark: bookmark.feed,
    search: search.feed,
    // Shared maps (read by SourceCard via the view).
    coverMap: shared.coverMap,
    statusMap: shared.statusMap,
    searchKeyword,
    loadMore,
    reload,
    setStatus,
    setSearchKeyword,
    resetAll,
  };
});
