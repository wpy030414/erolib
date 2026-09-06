import { create } from 'zustand';
import { api } from '@/services/api';
import { createBrowseFeed, type BrowseFeedShared } from '@/services/browse-feed';
import type { PixivWork, PixivBrowseStatus } from '@/types';

export type PixivTab = 'recommend' | 'following' | 'bookmark';
type FeedKey = PixivTab | 'search';

// Following & search return a fixed per-page from Pixiv (~30); this is the
// end-of-feed heuristic for those (a short page ⇒ last page). The grid's
// actual page size is 48 — see browse-feed's BROWSE_PAGE_SIZE.
const SOURCE_END_HINT = 30;
// Bookmark limit is tunable, so fetch as much as Pixiv allows per request —
// the 48/page buffer then needs fewer round-trips, and any surplus past a
// page boundary stays buffered for the next loadMore. 100 is within range.
const BOOKMARK_FETCH = 100;

interface PixivBrowseState {
  searchKeyword: string;
  recommend: { items: PixivWork[]; loading: boolean; end: boolean };
  following: { items: PixivWork[]; loading: boolean; end: boolean };
  bookmark: { items: PixivWork[]; loading: boolean; end: boolean };
  search: { items: PixivWork[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>;
  statusMap: Record<string, PixivBrowseStatus>;
  loadMore: (target: FeedKey) => Promise<void>;
  reload: (target: FeedKey) => Promise<void>;
  setStatus: (workId: string, status: PixivBrowseStatus) => void;
  setSearchKeyword: (kw: string) => void;
  resetAll: () => void;
}

/**
 * Pixiv browse store — a thin instantiation layer over the browse-feed
 * kernel.
 *
 * The four feeds (recommend / following / bookmark / search) each inject
 * their own fetchPage (that's where the per-tab pagination model lives:
 * recommend is one-shot, bookmark is offset/total, following & search are
 * page-based), but they SHARE one coverMap / statusMap / coverLoading — the
 * same Pixiv work routinely appears in several tabs, so a cover or download
 * state resolved in one must be visible in the others. Only the recommend
 * instance arms the progress listener for the group. */
export const usePixivBrowseStore = create<PixivBrowseState>((set, get) => {
  const searchKeyword = { value: '' };

  const shared: BrowseFeedShared<PixivBrowseStatus> = { coverMap: {}, statusMap: {}, coverLoading: new Set() };

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
  const recommend = createBrowseFeed<PixivWork, string, PixivBrowseStatus, number>({
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
  const following = createBrowseFeed<PixivWork, string, PixivBrowseStatus, number>({
    ...common,
    initialCursor: 1,
    fetchPage: async (cursor) => {
      const items = await api.listPixivFollowingFeed(cursor);
      return { items, nextCursor: cursor + 1, end: items.length < SOURCE_END_HINT };
    },
  });

  // 收藏: offset-paginated with a known total.
  const bookmark = createBrowseFeed<PixivWork, string, PixivBrowseStatus, number>({
    ...common,
    initialCursor: 0,
    fetchPage: async (cursor) => {
      const pageRes = await api.listPixivBookmarks(cursor, BOOKMARK_FETCH);
      const nextCursor = cursor + pageRes.items.length;
      return { items: pageRes.items, nextCursor, end: nextCursor >= pageRes.total };
    },
  });

  // 搜索: page-based like following, driven by `searchKeyword`.
  const search = createBrowseFeed<PixivWork, string, PixivBrowseStatus, number>({
    ...common,
    initialCursor: 1,
    fetchPage: async (cursor) => {
      const kw = searchKeyword.value.trim();
      if (!kw) return { items: [], nextCursor: cursor, end: true };
      const items = await api.searchPixivIllusts(kw, cursor);
      return { items, nextCursor: cursor + 1, end: items.length < SOURCE_END_HINT };
    },
  });

  // Mirror each kernel's feed object into this hook store so views re-render.
  for (const [key, inst] of [['recommend', recommend], ['following', following], ['bookmark', bookmark], ['search', search]] as const) {
    inst.subscribe(() => { set({ [key]: inst.getState().feed } as Partial<PixivBrowseState>); });
  }

  function instOf(target: FeedKey) {
    if (target === 'search') return search;
    if (target === 'recommend') return recommend;
    if (target === 'following') return following;
    return bookmark;
  }

  return {
    // Per-tab feed state (bind to <FeedList feed={...}>).
    recommend: recommend.getState().feed,
    following: following.getState().feed,
    bookmark: bookmark.getState().feed,
    search: search.getState().feed,
    // Shared maps (read by SourceCard via the view).
    coverMap: shared.coverMap,
    statusMap: shared.statusMap,
    searchKeyword: '',

    loadMore: (target) => instOf(target).loadMore(),
    reload: (target) => instOf(target).reload(),
    /** Optimistically mark a work as downloading (the view does this right
     *  after enqueuing so the mask shows before the first progress tick).
     *  Writes to the shared statusMap, so the state is visible on every tab
     *  that shows this work. */
    setStatus: (workId, status) => recommend.setStatus(workId, status),
    /** Commit the search box text: reset the search feed and fire the first
     *  page. An empty query clears the search (back to recommend). */
    setSearchKeyword: (kw) => {
      const trimmed = kw.trim();
      searchKeyword.value = trimmed;
      set({ searchKeyword: trimmed });
      search.resetFeed();
      if (trimmed) void search.loadMore();
    },
    /** Log-out: drop every feed's items + cursor and clear the shared
     *  statusMap (a different account may have different local state).
     *  coverMap is kept — IndexedDB-backed covers survive across logins. */
    resetAll: () => {
      searchKeyword.value = '';
      set({ searchKeyword: '' });
      recommend.resetFeed();
      following.resetFeed();
      bookmark.resetFeed();
      search.resetFeed();
      recommend.clearStatusMap();
    },
  };
});
