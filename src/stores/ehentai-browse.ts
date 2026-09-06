import { create } from 'zustand';
import { api } from '@/services/api';
import { createBrowseFeed } from '@/services/browse-feed';
import type { GalleryListItem, EhentaiBrowseStatus } from '@/types';

const EX_KEY = 'erolib.ehentai.ex';
/** Default category path segment: Doujinshi. */
const DEFAULT_CATEGORY = 'doujinshi';
/** A short results page means we've reached the tail of the listing. */
const PAGE_HINT = 25;

function readEx(): boolean {
  try {
    return localStorage.getItem(EX_KEY) === '1';
  } catch {
    return false;
  }
}

/** Canonical gallery URL for an item, honouring the current EX mode. This is
 *  the stable key used across statusMap and the task payload. (coverMap is
 *  keyed by gid instead so covers survive an EX toggle.) */
function galleryUrlOf(ex: boolean, item: GalleryListItem): string {
  const host = ex ? 'exhentai' : 'e-hentai';
  return `https://${host}.org/g/${item.gid}/${item.token}/`;
}

interface EhentaiBrowseState {
  feed: { items: GalleryListItem[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>;
  statusMap: Record<string, EhentaiBrowseStatus>;
  category: string | null;
  keyword: string;
  ex: boolean;
  galleryUrlOf: (item: GalleryListItem) => string;
  loadMore: () => Promise<void>;
  reload: () => Promise<void>;
  resetAll: () => void;
  setStatus: (galleryUrl: string, status: EhentaiBrowseStatus) => void;
  setKeyword: (kw: string) => void;
  setEx: (v: boolean) => void;
  selectCategory: (path: string | null) => void;
}

/**
 * EHentai browse store — one browse-feed instance (search has a single result
 * list, paginated by a gid cursor). Source-specific bits (the gallery URL
 * builder that honours EX mode, category/keyword/EX preferences) live here;
 * pagination + covers + status + the progress listener come from the kernel. */
export const useEhentaiBrowseStore = create<EhentaiBrowseState>((set, get) => {
  const inst = createBrowseFeed<GalleryListItem, string, EhentaiBrowseStatus, string | null>({
    keyOf: (item) => galleryUrlOf(get().ex, item),
    statusKeyOf: (s) => s.galleryUrl,
    // gid — stable across EX toggles, matches the IndexedDB key, shared with
    // the library (source_post_id = gid).
    coverKeyOf: (item) => item.gid,
    coverUrlOf: (item) => item.thumbUrl,
    fetchStatus: (urls) => api.ehentaiBrowseStatus(urls),
    proxyCover: (url) => api.ehentaiProxyThumb(url),
    initialCursor: null,
    listen: true,
    fetchPage: async (cursor) => {
      const list = await api.ehentaiSearch(
        get().keyword || null,
        get().category,
        cursor,
        get().ex,
      );
      // e-hentai's next cursor is the gid of the last gallery on this page.
      return {
        items: list,
        nextCursor: list.length ? list[list.length - 1].gid : null,
        end: list.length < PAGE_HINT,
      };
    },
  });

  // Mirror the kernel's feed object into this hook store so views re-render.
  inst.subscribe(() => { set({ feed: inst.getState().feed }); });

  return {
    feed: inst.getState().feed,
    coverMap: inst.coverMap,
    statusMap: inst.statusMap,
    category: DEFAULT_CATEGORY,
    keyword: '',
    ex: readEx(),
    galleryUrlOf: (item) => galleryUrlOf(get().ex, item),

    loadMore: () => inst.loadMore(),
    /** Drop everything (items/covers/status) and fetch fresh. Used on search,
     *  category, and EX-mode changes. Clears statusMap wholesale because an
     *  EX toggle re-keys every gallery URL (stale entries would leak). */
    reload: async () => {
      inst.clearStatusMap();
      await inst.reload();
    },
    /** Log-out: drop items + browse status + pagination cursor (covers stay
     *  cached in IndexedDB). Keyword/category/EX preference are kept so the
     *  next login resumes with the same query. */
    resetAll: () => {
      inst.resetFeed();
      inst.clearStatusMap();
    },
    setStatus: (galleryUrl, status) => inst.setStatus(galleryUrl, status),
    setKeyword: (kw) => set({ keyword: kw }),
    /** Toggle EX mode and persist. The caller follows up with reload(). */
    setEx: (v) => {
      set({ ex: v });
      try {
        localStorage.setItem(EX_KEY, v ? '1' : '0');
      } catch {
        // ignore storage errors
      }
    },
    /** Single-select a category path (null = all categories); reloads. */
    selectCategory: (path) => {
      set({ category: path });
      void get().reload();
    },
  };
});
