import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getThumb, setThumb } from '@/services/thumb-cache';
import { api } from '@/services/api';
import type { GalleryListItem, EhentaiBrowseStatus } from '@/types';

const BROWSE_PAGE_SIZE = 48;
const PAGE_HINT = 25;
const EX_KEY = 'erolib.ehentai.ex';

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
  setEx: (v: boolean) => void;
  selectCategory: (path: string | null) => void;
}

let cursor: string | null = null;
let sourceEnded = false;
let loading = false;
let seenKeys = new Set<string>();
let buffer: GalleryListItem[] = [];
let coverLoading = new Set<string>();
let inFlight = 0;
const gateQueue: Array<() => void> = [];
let progressUnlisten: UnlistenFn | null = null;
let deletedUnlisten: UnlistenFn | null = null;

function gateEnter(): Promise<void> {
  if (inFlight < 6) { inFlight++; return Promise.resolve(); }
  return new Promise((r) => { gateQueue.push(() => { inFlight++; r(); }); });
}
function gateLeave() { inFlight--; const n = gateQueue.shift(); if (n) n(); }

async function loadCover(gid: string, thumbUrl: string | null, coverMap: Record<string, string | null>) {
  if (coverMap[gid] !== undefined) return;
  if (!thumbUrl) { coverMap[gid] = null; return; }
  coverLoading.add(gid); coverMap[gid] = null;
  try {
    await gateEnter();
    let blob = await getThumb(gid);
    if (!blob) { const bytes = await api.ehentaiProxyThumb(thumbUrl); blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }); void setThumb(gid, blob); }
    if (coverLoading.has(gid)) { const old = coverMap[gid]; if (old) URL.revokeObjectURL(old); coverMap[gid] = URL.createObjectURL(blob); coverLoading.delete(gid); }
  } catch { coverMap[gid] = null; coverLoading.delete(gid); }
  finally { gateLeave(); }
}

export const useEhentaiBrowseStore = create<EhentaiBrowseState>((set, get) => ({
  feed: { items: [], loading: false, end: false }, coverMap: {}, statusMap: {}, category: 'doujinshi', keyword: '',
  ex: (() => { try { return window.localStorage.getItem(EX_KEY) === '1'; } catch { return false; } })(),
  galleryUrlOf: (item) => { const host = get().ex ? 'exhentai.org' : 'e-hentai.org'; return `https://${host}/g/${item.gid}/${item.token}/`; },
  loadMore: async () => {
    if (loading || sourceEnded) return;
    loading = true; set((s) => ({ feed: { ...s.feed, loading: true } }));
    try {
      while (buffer.length < BROWSE_PAGE_SIZE && !sourceEnded) {
        const { items, nextCursor, end } = await api.ehentaiSearch(get().keyword, get().category ?? undefined, cursor, get().ex);
        for (const item of items) { const key = get().galleryUrlOf(item); if (!seenKeys.has(key)) { seenKeys.add(key); buffer.push(item); } }
        cursor = nextCursor;
        if (end || items.length < PAGE_HINT) { sourceEnded = true; break; }
      }
      const pageItems = buffer.splice(0, BROWSE_PAGE_SIZE);
      if (pageItems.length > 0) {
        set((s) => { const newItems = [...s.feed.items, ...pageItems]; for (const item of pageItems) void loadCover(item.gid, item.thumbUrl, s.coverMap); return { feed: { items: newItems, loading: false, end: sourceEnded && buffer.length === 0 } }; });
        const urls = pageItems.map((i) => get().galleryUrlOf(i));
        void api.ehentaiBrowseStatus(urls).then((statuses) => { set((s) => { const sm = { ...s.statusMap }; for (const st of statuses) sm[st.galleryUrl] = st; return { statusMap: sm }; }); }).catch(() => {});
      } else { set((s) => ({ feed: { ...s.feed, loading: false, end: true } })); }
    } catch { set((s) => ({ feed: { ...s.feed, loading: false } })); }
    finally { loading = false; }
  },
  reload: async () => { cursor = null; sourceEnded = false; seenKeys = new Set(); buffer = []; set((s) => ({ feed: { items: [], loading: false, end: false }, statusMap: {} })); await get().loadMore(); },
  resetAll: () => { cursor = null; sourceEnded = false; seenKeys = new Set(); buffer = []; set((s) => ({ feed: { items: [], loading: false, end: false }, statusMap: {} })); },
  setStatus: (galleryUrl, status) => { set((s) => ({ statusMap: { ...s.statusMap, [galleryUrl]: status } })); },
  setEx: (v) => { try { window.localStorage.setItem(EX_KEY, v ? '1' : ''); } catch { /* ignore */ } set({ ex: v }); void get().reload(); },
  selectCategory: (path) => { set({ category: path }); void get().reload(); },
}));

void (async () => {
  progressUnlisten = await listen<{ task_id: string; status: string; progress_current: number; progress_total: number }>('task://progress', (event) => {
    const p = event.payload; const store = useEhentaiBrowseStore.getState();
    let found: string | null = null;
    for (const [key, s] of Object.entries(store.statusMap)) { if (s.taskId === p.task_id) { found = key; break; } }
    if (found) { useEhentaiBrowseStore.setState((s) => ({ statusMap: { ...s.statusMap, [found!]: { ...s.statusMap[found!], taskStatus: p.status, progressCurrent: p.progress_current, progressTotal: p.progress_total } } })); }
  });
  deletedUnlisten = await listen<{ book_id: string }>('book://deleted', (event) => {
    useEhentaiBrowseStore.setState((s) => {
      const sm = { ...s.statusMap }; let changed = false;
      for (const [key, st] of Object.entries(sm)) { if (st.localBookId === event.payload.book_id) { sm[key] = { ...st, localBookId: undefined }; changed = true; } }
      return changed ? { statusMap: sm } : {};
    });
  });
})();
