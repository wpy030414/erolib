import { defineStore } from 'pinia';
import { reactive, ref, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '@/services/api';
import { getThumb, setThumb } from '@/services/thumb-cache';
import type { GalleryListItem, EhentaiBrowseStatus } from '@/types';
import type { TaskItem } from '@/stores/tasks';

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

export const useEhentaiBrowseStore = defineStore('ehentai-browse', () => {
  const items = ref<GalleryListItem[]>([]);
  const loading = ref(false);
  const end = ref(false);
  /** Pagination cursor — gid of the last fetched gallery (null = first page).
   *  e-hentai paginates with `?next={gid}`, not `?page=N` (page 0/1/2 return
   *  identical results). */
  const nextCursor = ref<string | null>(null);
  /** Category path segment (e.g. "doujinshi"); null = all categories. */
  const category = ref<string | null>(DEFAULT_CATEGORY);
  const keyword = ref('');
  /** EX mode (exhentai.org) — persisted, toggled from the view header. */
  const ex = ref(readEx());

  const coverMap = reactive<Record<string, string | null>>({});
  const statusMap = reactive<Record<string, EhentaiBrowseStatus>>({});
  const coverLoading = new Set<string>();

  /** Canonical gallery URL for an item, honouring the current EX mode. This is
   *  the stable key used across coverMap/statusMap and the task payload. */
  function galleryUrlOf(item: GalleryListItem): string {
    const host = ex.value ? 'exhentai' : 'e-hentai';
    return `https://${host}.org/g/${item.gid}/${item.token}/`;
  }

  async function refreshStatus(urls: string[]) {
    if (!urls.length) return;
    try {
      const list = await api.ehentaiBrowseStatus(urls);
      for (const s of list) statusMap[s.galleryUrl] = s;
    } catch (e) {
      console.error('refresh ehentai browse status:', e);
    }
  }

  async function loadMore() {
    if (loading.value || end.value) return;
    loading.value = true;
    try {
      const list = await api.ehentaiSearch(
        keyword.value || null,
        category.value,
        nextCursor.value,
        ex.value,
      );
      if (list.length === 0) {
        end.value = true;
      } else {
        // Resolve local state first so cards render correctly (no flash).
        await refreshStatus(list.map(galleryUrlOf));
        items.value.push(...list);
        // e-hentai's next cursor is the gid of the last gallery on this page.
        nextCursor.value = list[list.length - 1].gid;
        if (list.length < PAGE_HINT) end.value = true;
      }
    } catch (e) {
      console.error('ehentai loadMore:', e);
      if (items.value.length === 0) end.value = true;
    } finally {
      loading.value = false;
    }
  }

  /** Fetch a cover via the backend proxy (e-hentai blocks hotlinking from the
   *  WKWebView). coverMap is keyed by gid — stable across EX toggles, matches
   *  the IndexedDB key, and is shared with the library (source_post_id = gid). */
  async function loadCover(item: GalleryListItem) {
    const key = item.gid;
    if (!item.thumbUrl || key in coverMap || coverLoading.has(key)) return;
    coverLoading.add(key);
    coverMap[key] = null;
    try {
      // IndexedDB cache keyed by gid so repeat covers load instantly.
      let blob = await getThumb(key);
      if (!blob) {
        const bytes = await api.ehentaiProxyThumb(item.thumbUrl);
        blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
        void setThumb(key, blob);
      }
      coverMap[key] = URL.createObjectURL(blob);
    } catch {
      coverMap[key] = null;
    } finally {
      coverLoading.delete(key);
    }
  }

  function setStatus(galleryUrl: string, status: EhentaiBrowseStatus) {
    statusMap[galleryUrl] = status;
  }

  /** Patch the gallery bound to a task id; returns that gallery URL (or null). */
  function updateByTaskId(
    taskId: string,
    patch: Partial<EhentaiBrowseStatus>,
  ): string | null {
    for (const [gurl, st] of Object.entries(statusMap)) {
      if (st.taskId === taskId) {
        statusMap[gurl] = { ...st, ...patch };
        return gurl;
      }
    }
    return null;
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

  /** Drop everything (items/covers/status) and fetch fresh. Used on search,
   *  category, and EX-mode changes, plus the manual reload button. Clears the
   *  maps wholesale so EX-mode toggles (which re-key every gallery URL) leak
   *  nothing. */
  async function reload() {
    // coverMap (blob URLs) intentionally KEPT — covers stay cached across
    // reloads (IndexedDB-backed), no re-fetch flicker.
    Object.keys(statusMap).forEach((k) => delete statusMap[k]);
    items.value.splice(0, items.value.length);
    nextCursor.value = null;
    end.value = false;
    await loadMore();
  }

  // Auto-load covers whenever items stream in.
  watch(() => items.value.length, () => items.value.forEach(loadCover));

  // Track task progress at the store level (not the component) so card state
  // keeps updating even while the EHentai view is unmounted — a download that
  // finishes while the user is elsewhere still flips the card to "downloaded".
  // On a terminal status, re-resolve the gallery (completed → local book).
  const TERMINAL = ['completed', 'failed', 'cancelled'];
  let unlistenProgress: UnlistenFn | undefined;
  void listen<TaskItem>('task://progress', (event) => {
    const p = event.payload;
    const gurl = updateByTaskId(p.id, {
      taskStatus: p.status,
      progressCurrent: p.progress_current,
      progressTotal: p.progress_total,
    });
    if (gurl && TERMINAL.includes(p.status)) refreshStatus([gurl]);
  }).then((fn) => {
    unlistenProgress = fn;
  });

  /** Single-select a category path (null = all categories); reloads. */
  function selectCategory(path: string | null) {
    category.value = path;
    void reload();
  }

  /** Log-out: drop items + browse status + pagination cursor (covers stay
   *  cached in IndexedDB). Keyword/category/EX preference are kept so the next
   *  login resumes with the same query. */
  function resetAll() {
    Object.keys(statusMap).forEach((k) => delete statusMap[k]);
    items.value.splice(0, items.value.length);
    nextCursor.value = null;
    end.value = false;
  }

  return {
    items,
    loading,
    end,
    nextCursor,
    category,
    keyword,
    ex,
    coverMap,
    statusMap,
    galleryUrlOf,
    loadMore,
    reload,
    refreshStatus,
    loadCover,
    setStatus,
    updateByTaskId,
    setEx,
    selectCategory,
    resetAll,
  };
});
