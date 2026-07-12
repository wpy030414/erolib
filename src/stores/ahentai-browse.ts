import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/services/api';
import { useBrowseFeed } from '@/composables/useBrowseFeed';
import type { AhentaiGalleryItem, AhentaiBrowseStatus } from '@/types';

/** asmhentai.com serves 20 items per source page. A page with fewer than 20
 *  items means we've reached the end of the listing. */
const PAGE_HINT = 20;

/**
 * AHentai browse store — one `useBrowseFeed` instance (simple page-based
 * pagination, no login, no categories, no EX mode). Source-specific bits
 * (the keyword ref) live here; pagination + covers + status + the progress
 * listener come from the composable. */
export const useAhentaiBrowseStore = defineStore('ahentai-browse', () => {
  const keyword = ref('');

  const inst = useBrowseFeed<AhentaiGalleryItem, string, AhentaiBrowseStatus, number>({
    keyOf: (item) => item.id,
    statusKeyOf: (s) => s.galleryId,
    coverKeyOf: (item) => item.id,
    coverUrlOf: (item) => item.thumbUrl,
    fetchStatus: (ids) => api.ahentaiBrowseStatus(ids),
    proxyCover: (url) => api.ahentaiProxyThumb(url),
    initialCursor: 1,
    fetchPage: async (cursor) => {
      const list = await api.ahentaiSearch(keyword.value || null, cursor);
      return {
        items: list,
        nextCursor: cursor + 1,
        end: list.length < PAGE_HINT,
      };
    },
  });

  function loadMore() {
    return inst.loadMore();
  }

  /** Drop everything (items/covers-status) and fetch fresh. Used on search
   *  changes. */
  async function reload() {
    inst.clearStatusMap();
    await inst.reload();
  }

  function setStatus(galleryId: string, status: AhentaiBrowseStatus) {
    return inst.setStatus(galleryId, status);
  }

  /** Reset feed + clear statusMap (used on view unmount if needed). */
  function resetAll() {
    inst.resetFeed();
    inst.clearStatusMap();
  }

  return {
    feed: inst.feed,
    coverMap: inst.coverMap,
    statusMap: inst.statusMap,
    keyword,
    loadMore,
    reload,
    setStatus,
    resetAll,
  };
});
