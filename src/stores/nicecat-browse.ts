import { defineStore } from 'pinia';
import { computed, reactive, ref } from 'vue';
import { api } from '@/services/api';
import { useBrowseFeed } from '@/composables/useBrowseFeed';
import type { NicecatBrowseStatus, NicecatComicItem } from '@/types';

const PAGE_HINT = 60; // searchTag returns up to 60 items per page

export const useNicecatBrowseStore = defineStore('nicecat-browse', () => {
  const keyword = ref('');

  // ---- homepage state (still hand-rolled — no cursor model needed) ----
  const sections = ref<{ name: string; comics: NicecatComicItem[] }[]>([]);
  const cachedSections = ref<{ name: string; comics: NicecatComicItem[] }[]>([]);
  const homeLoading = ref(false);
  const homeError = ref<string | null>(null);

  // ---- search feed (via useBrowseFeed — same as AHentai) ----
  const search = useBrowseFeed<
    NicecatComicItem,
    string,
    NicecatBrowseStatus,
    string // cursor = searchId (page 1), then searchId for subsequent pages
  >({
    keyOf: (item) => item.uid,
    statusKeyOf: (s) => s.comicId,
    coverKeyOf: (item) => item.uid,
    coverUrlOf: (item) => item.image ?? null,
    fetchStatus: (ids) => api.nicecatBrowseStatus(ids),
    proxyCover: (url) => api.nicecatProxyThumb(url),
    initialCursor: '', // empty = start fresh
    fetchPage: async (cursor) => {
      const kw = keyword.value.trim();
      if (!kw) return { items: [], nextCursor: cursor, end: true };
      const json = await api.nicecatFetchApi('/api/ComicSearch/search', {
        content: kw,
        cursor: cursor, // "" = page 1, non-empty = searchId
      });
      const data = (json as any).data ?? {};
      const list: NicecatComicItem[] = data.list ?? [];
      const nextCursor: string = data.nextCursor ?? '';
      return {
        items: list,
        nextCursor,
        // End when: server returns a short page, OR cursor is exhausted
        // (searchId empty → no more pages).  Without the nextCursor guard
        // the feed loops back to page 1 forever and freezes the UI.
        end: list.length < PAGE_HINT || !nextCursor,
      };
    },
  });

  const isSearching = computed(() => keyword.value.trim().length > 0);

  // Convenience: feed for <FeedList>
  const feed = search.feed;

  // ---- homepage ----

  interface RawHomeSection {
    ViewName: string;
    ViewDataArray: NicecatComicItem[];
  }

  interface RawRandomFeedData {
    homeData?: RawHomeSection[];
    tagData?: Array<{ uid: string; name: string; dataType: number }>;
    recommend?: NicecatComicItem;
  }

  const loaded = ref(false);

  async function loadHomepage() {
    if (homeLoading.value) return;
    if (loaded.value && sections.value.length > 0) return;
    homeLoading.value = true;
    homeError.value = null;
    try {
      const json = await api.nicecatFetchApi('/api/HomeFeed/randomFeed', {});
      const data = (json as any).data as RawRandomFeedData;
      sections.value = (data.homeData ?? []).map((raw) => ({
        name: raw.ViewName,
        comics: raw.ViewDataArray ?? [],
      }));
      cachedSections.value = sections.value;
      loaded.value = true;
      // Manually kick off cover loads for homepage items — the watch in
      // useBrowseFeed only observes search.feed.items, not homepage sections.
      for (const sec of sections.value) {
        for (const comic of sec.comics) search.loadCover(comic);
      }
      // Mirror search.loadMore(): homepage cards share the same statusMap, and
      // without this the per-card red-dot / downloaded overlay never appears
      // because loadHomepage() never goes through loadMore().
      const uids = sections.value.flatMap((s) => s.comics.map((c) => c.uid));
      if (uids.length) void search.refreshStatus(uids);
    } catch (e) {
      homeError.value = e instanceof Error ? e.message : String(e);
      console.error('nicecat homepage:', homeError.value);
      sections.value = [];
    } finally {
      homeLoading.value = false;
    }
  }

  // ---- reload & reset ----

  async function reload(force = false) {
    homeError.value = null;
    if (isSearching.value) {
      if (sections.value.length > 0) cachedSections.value = sections.value;
      loaded.value = false;
      await search.reload();
    } else if (!force && cachedSections.value.length > 0) {
      sections.value = cachedSections.value;
      loaded.value = true;
    } else {
      loaded.value = false;
      sections.value = [];
      await loadHomepage();
    }
  }

  function resetAll() {
    loaded.value = false;
    sections.value = [];
    cachedSections.value = [];
    homeLoading.value = false;
    homeError.value = null;
    search.resetFeed();
    search.clearStatusMap();
  }

  return {
    keyword,
    sections,
    homeLoading,
    homeError,
    feed,
    coverMap: search.coverMap,
    statusMap: search.statusMap,
    error: homeError,
    isSearching,
    loadHomepage,
    searchMore: () => search.loadMore(),
    reload,
    resetAll,
    setStatus: (comicId: string, s: NicecatBrowseStatus) => search.setStatus(comicId, s),
    isBusy: (comicId: string) => {
      const s = search.statusMap[comicId];
      const ACTIVE = ['pending', 'running', 'paused'];
      return !!s?.taskId && ACTIVE.includes(s.taskStatus ?? '');
    },
  };
});
