import { create } from 'zustand';
import { api } from '@/services/api';
import { createBrowseFeed } from '@/services/browse-feed';
import type { NicecatBrowseStatus, NicecatComicItem } from '@/types';

/** searchTag returns up to 60 items per page. */
const PAGE_HINT = 60;

interface HomeSection {
  name: string;
  comics: NicecatComicItem[];
}

interface RawHomeSection {
  ViewName: string;
  ViewDataArray: NicecatComicItem[];
}

interface RawRandomFeedData {
  homeData?: RawHomeSection[];
  tagData?: Array<{ uid: string; name: string; dataType: number }>;
  recommend?: NicecatComicItem;
}

const ACTIVE_STATUSES = ['pending', 'running', 'paused'];

interface NicecatBrowseState {
  keyword: string;
  sections: HomeSection[];
  homeLoading: boolean;
  homeError: string | null;
  feed: { items: NicecatComicItem[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>;
  statusMap: Record<string, NicecatBrowseStatus>;
  readonly isSearching: boolean;
  loaded: boolean;
  loadHomepage: () => Promise<void>;
  searchMore: () => Promise<void>;
  reload: (force?: boolean) => Promise<void>;
  resetAll: () => void;
  setStatus: (comicId: string, status: NicecatBrowseStatus) => void;
  setKeyword: (kw: string) => void;
  isBusy: (comicId: string) => boolean;
}

/**
 * NiceCat browse store — homepage stays hand-rolled (no cursor model), while
 * search runs on one browse-feed instance (cursor = searchId: '' fetches page
 * 1, the returned searchId pages on). Homepage cards and search results share
 * one coverMap / statusMap via the instance's maps. */
export const useNicecatBrowseStore = create<NicecatBrowseState>((set, get) => {
  const keyword = { value: '' };

  // ---- search feed (via the browse-feed kernel — same as AHentai) ----
  const search = createBrowseFeed<NicecatComicItem, string, NicecatBrowseStatus, string>({
    keyOf: (item) => item.uid,
    statusKeyOf: (s) => s.comicId,
    coverKeyOf: (item) => item.uid,
    coverUrlOf: (item) => item.image ?? null,
    fetchStatus: (ids) => api.nicecatBrowseStatus(ids),
    proxyCover: (url) => api.nicecatProxyThumb(url),
    initialCursor: '', // empty = start fresh
    listen: true,
    fetchPage: async (cursor) => {
      const kw = keyword.value.trim();
      if (!kw) return { items: [], nextCursor: cursor, end: true };
      const json = await api.nicecatFetchApi('/api/ComicSearch/search', {
        content: kw,
        cursor, // "" = page 1, non-empty = searchId
      });
      const data = (json as { data?: { list?: NicecatComicItem[]; nextCursor?: string } }).data ?? {};
      const list: NicecatComicItem[] = data.list ?? [];
      const nextCursor: string = data.nextCursor ?? '';
      return {
        items: list,
        nextCursor,
        // End when: server returns a short page, OR cursor is exhausted
        // (searchId empty → no more pages). Without the nextCursor guard the
        // feed loops back to page 1 forever and freezes the UI.
        end: list.length < PAGE_HINT || !nextCursor,
      };
    },
  });

  // Mirror the kernel's feed object into this hook store so views re-render.
  search.subscribe(() => { set({ feed: search.getState().feed }); });

  // ---- homepage state ----
  let cachedSections: HomeSection[] = [];

  async function loadHomepage() {
    if (get().homeLoading) return;
    if (get().loaded && get().sections.length > 0) return;
    set({ homeLoading: true, homeError: null });
    try {
      const json = await api.nicecatFetchApi('/api/HomeFeed/randomFeed', {});
      const data = (json as { data?: RawRandomFeedData }).data ?? {};
      const sections: HomeSection[] = (data.homeData ?? []).map((raw) => ({
        name: raw.ViewName,
        comics: raw.ViewDataArray ?? [],
      }));
      cachedSections = sections;
      set({ sections, homeLoading: false, loaded: true });
      // Manually kick off cover loads for homepage items — the kernel only
      // covers items pushed through loadMore, not homepage sections.
      for (const sec of sections) {
        for (const comic of sec.comics) void search.loadCover(comic);
      }
      // Mirror the kernel's loadMore(): homepage cards share the same
      // statusMap, and without this the per-card red-dot / downloaded overlay
      // never appears because loadHomepage() never goes through loadMore().
      const uids = sections.flatMap((s) => s.comics.map((c) => c.uid));
      if (uids.length) void search.refreshStatus(uids);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      console.error('nicecat homepage:', message);
      set({ homeError: message, homeLoading: false, sections: [] });
    }
  }

  return {
    keyword: '',
    sections: [],
    homeLoading: false,
    homeError: null,
    feed: search.getState().feed,
    coverMap: search.coverMap,
    statusMap: search.statusMap,
    get isSearching() { return get().keyword.trim().length > 0; },
    loaded: false,

    loadHomepage,
    searchMore: () => search.loadMore(),
    /** Search mode: cache the homepage, re-run the search feed from scratch.
     *  Browse mode: restore cached sections, or (re)load the homepage. */
    reload: async (force = false) => {
      set({ homeError: null });
      if (get().isSearching) {
        if (get().sections.length > 0) cachedSections = get().sections;
        set({ loaded: false });
        await search.reload();
      } else if (!force && cachedSections.length > 0) {
        set({ sections: cachedSections, loaded: true });
      } else {
        set({ loaded: false, sections: [] });
        await loadHomepage();
      }
    },
    resetAll: () => {
      cachedSections = [];
      set({ loaded: false, sections: [], homeLoading: false, homeError: null });
      search.resetFeed();
      search.clearStatusMap();
    },
    setStatus: (comicId, status) => search.setStatus(comicId, status),
    setKeyword: (kw) => { keyword.value = kw; set({ keyword: kw }); },
    isBusy: (comicId) => {
      const s = get().statusMap[comicId];
      return !!s?.taskId && ACTIVE_STATUSES.includes(s.taskStatus ?? '');
    },
  };
});
