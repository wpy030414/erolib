import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/services/api';
import { useBrowseFeed } from '@/composables/useBrowseFeed';
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

/**
 * EHentai browse store — one `useBrowseFeed` instance (search has a single
 * result list, paginated by a gid cursor). Source-specific bits (the gallery
 * URL builder that honours EX mode, category/keyword/EX preferences) live
 * here; pagination + covers + status + the progress listener come from the
 * composable. */
export const useEhentaiBrowseStore = defineStore('ehentai-browse', () => {
  /** Pagination cursor — gid of the last fetched gallery (null = first page).
   *  e-hentai paginates with `?next={gid}`, not `?page=N` (page 0/1/2 return
   *  identical results). */
  /** Category path segment (e.g. "doujinshi"); null = all categories. */
  const category = ref<string | null>(DEFAULT_CATEGORY);
  const keyword = ref('');
  /** EX mode (exhentai.org) — persisted, toggled from the view header. */
  const ex = ref(readEx());

  /** Canonical gallery URL for an item, honouring the current EX mode. This is
   *  the stable key used across statusMap and the task payload. (coverMap is
   *  keyed by gid instead — see coverKeyOf — so covers survive an EX toggle.) */
  function galleryUrlOf(item: GalleryListItem): string {
    const host = ex.value ? 'exhentai' : 'e-hentai';
    return `https://${host}.org/g/${item.gid}/${item.token}/`;
  }

  const inst = useBrowseFeed<GalleryListItem, string, EhentaiBrowseStatus, string | null>({
    keyOf: galleryUrlOf,
    statusKeyOf: (s) => s.galleryUrl,
    // gid — stable across EX toggles, matches the IndexedDB key, shared with
    // the library (source_post_id = gid).
    coverKeyOf: (item) => item.gid,
    coverUrlOf: (item) => item.thumbUrl,
    fetchStatus: (urls) => api.ehentaiBrowseStatus(urls),
    proxyCover: (url) => api.ehentaiProxyThumb(url),
    initialCursor: null,
    fetchPage: async (cursor) => {
      const list = await api.ehentaiSearch(
        keyword.value || null,
        category.value,
        cursor,
        ex.value,
      );
      // e-hentai's next cursor is the gid of the last gallery on this page.
      return {
        items: list,
        nextCursor: list.length ? list[list.length - 1].gid : null,
        end: list.length < PAGE_HINT,
      };
    },
  });

  function loadMore() {
    return inst.loadMore();
  }

  /** Drop everything (items/covers-status) and fetch fresh. Used on search,
   *  category, and EX-mode changes. Clears statusMap wholesale because an EX
   *  toggle re-keys every gallery URL (stale entries would otherwise leak). */
  async function reload() {
    inst.clearStatusMap();
    await inst.reload();
  }

  function setStatus(galleryUrl: string, status: EhentaiBrowseStatus) {
    return inst.setStatus(galleryUrl, status);
  }

  /** Toggle EX mode and persist. The caller follows up with reload(). */
  function setEx(v: boolean) {
    ex.value = v;
    try {
      localStorage.setItem(EX_KEY, v ? '1' : '0');
    } catch {
      // ignore storage errors
    }
  }

  /** Single-select a category path (null = all categories); reloads. */
  function selectCategory(path: string | null) {
    category.value = path;
    void reload();
  }

  /** Log-out: drop items + browse status + pagination cursor (covers stay
   *  cached in IndexedDB). Keyword/category/EX preference are kept so the next
   *  login resumes with the same query. */
  function resetAll() {
    inst.resetFeed();
    inst.clearStatusMap();
  }

  return {
    feed: inst.feed,
    coverMap: inst.coverMap,
    statusMap: inst.statusMap,
    category,
    keyword,
    ex,
    galleryUrlOf,
    loadMore,
    reload,
    setStatus,
    setEx,
    selectCategory,
    resetAll,
  };
});
