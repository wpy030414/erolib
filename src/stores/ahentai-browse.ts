import { create } from 'zustand';
import { api } from '@/services/api';
import { createBrowseFeed } from '@/services/browse-feed';
import type { AhentaiGalleryItem, AhentaiBrowseStatus } from '@/types';

/** asmhentai.com serves 20 items per source page. A page with fewer than 20
 *  items means we've reached the end of the listing. */
const PAGE_HINT = 20;

interface AhentaiBrowseState {
  feed: { items: AhentaiGalleryItem[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>;
  statusMap: Record<string, AhentaiBrowseStatus>;
  keyword: string;
  loadMore: () => Promise<void>;
  reload: () => Promise<void>;
  resetAll: () => void;
  setStatus: (galleryId: string, status: AhentaiBrowseStatus) => void;
  setKeyword: (kw: string) => void;
}

/**
 * AHentai browse store — one browse-feed instance (simple page-based
 * pagination, no login, no categories, no EX mode). Source-specific bits
 * (the keyword) live here; pagination + covers + status + the progress
 * listener come from the kernel. */
export const useAhentaiBrowseStore = create<AhentaiBrowseState>((set, get) => {
  const keyword = { value: '' };

  const inst = createBrowseFeed<AhentaiGalleryItem, string, AhentaiBrowseStatus, number>({
    keyOf: (item) => item.id,
    statusKeyOf: (s) => s.galleryId,
    coverKeyOf: (item) => item.id,
    coverUrlOf: (item) => item.thumbUrl,
    fetchStatus: (ids) => api.ahentaiBrowseStatus(ids),
    proxyCover: (url) => api.ahentaiProxyThumb(url),
    initialCursor: 1,
    listen: true,
    fetchPage: async (cursor) => {
      const list = await api.ahentaiSearch(keyword.value || null, cursor);
      return {
        items: list,
        nextCursor: cursor + 1,
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
    keyword: '',

    loadMore: () => inst.loadMore(),
    /** Drop everything (items/covers-status) and fetch fresh. Used on search
     *  changes. */
    reload: async () => {
      inst.clearStatusMap();
      await inst.reload();
    },
    /** Reset feed + clear statusMap. */
    resetAll: () => {
      inst.resetFeed();
      inst.clearStatusMap();
    },
    setStatus: (galleryId, status) => inst.setStatus(galleryId, status),
    setKeyword: (kw) => { keyword.value = kw; set({ keyword: kw }); },
  };
});
