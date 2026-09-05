import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getThumb, setThumb } from '@/services/thumb-cache';
import { api } from '@/services/api';
import type { AhentaiGalleryItem, AhentaiBrowseStatus } from '@/types';

const BROWSE_PAGE_SIZE = 48; const PAGE_HINT = 20;
let page = 1; let sourceEnded = false; let loading = false;
let seenKeys = new Set<string>(); let buffer: AhentaiGalleryItem[] = [];
let coverLoading = new Set<string>(); let inFlight = 0;
const gateQueue: Array<() => void> = [];

function gateEnter(): Promise<void> { if (inFlight < 6) { inFlight++; return Promise.resolve(); } return new Promise((r) => { gateQueue.push(() => { inFlight++; r(); }); }); }
function gateLeave() { inFlight--; const n = gateQueue.shift(); if (n) n(); }

async function loadCover(galleryId: string, coverUrl: string | null, coverMap: Record<string, string | null>) {
  if (coverMap[galleryId] !== undefined || !coverUrl) { if (!coverUrl) coverMap[galleryId] = null; return; }
  coverLoading.add(galleryId); coverMap[galleryId] = null;
  try { await gateEnter(); let blob = await getThumb(galleryId); if (!blob) { const bytes = await api.ahentaiProxyThumb(coverUrl); blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }); void setThumb(galleryId, blob); } if (coverLoading.has(galleryId)) { const old = coverMap[galleryId]; if (old) URL.revokeObjectURL(old); coverMap[galleryId] = URL.createObjectURL(blob); coverLoading.delete(galleryId); } } catch { coverMap[galleryId] = null; coverLoading.delete(galleryId); } finally { gateLeave(); }
}

interface AhentaiBrowseState {
  feed: { items: AhentaiGalleryItem[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>; statusMap: Record<string, AhentaiBrowseStatus>; keyword: string;
  loadMore: () => Promise<void>; reload: () => Promise<void>; resetAll: () => void;
  setStatus: (galleryId: string, status: AhentaiBrowseStatus) => void;
}

export const useAhentaiBrowseStore = create<AhentaiBrowseState>((set, get) => ({
  feed: { items: [], loading: false, end: false }, coverMap: {}, statusMap: {}, keyword: '',

  loadMore: async () => {
    if (loading || sourceEnded) return; loading = true; set((s) => ({ feed: { ...s.feed, loading: true } }));
    try {
      while (buffer.length < BROWSE_PAGE_SIZE && !sourceEnded) { const items = await api.ahentaiSearch(get().keyword, page); for (const item of items) { if (!seenKeys.has(item.id)) { seenKeys.add(item.id); buffer.push(item); } } page++; if (items.length < PAGE_HINT) { sourceEnded = true; break; } }
      const pageItems = buffer.splice(0, BROWSE_PAGE_SIZE);
      if (pageItems.length > 0) { set((s) => { const newItems = [...s.feed.items, ...pageItems]; for (const item of pageItems) void loadCover(item.id, item.thumbUrl, s.coverMap); return { feed: { items: newItems, loading: false, end: sourceEnded && buffer.length === 0 } }; }); void api.ahentaiBrowseStatus(pageItems.map((i) => i.id)).then((statuses) => { set((s) => { const sm = { ...s.statusMap }; for (const st of statuses) sm[st.galleryId] = st; return { statusMap: sm }; }); }).catch(() => {}); }
      else set((s) => ({ feed: { ...s.feed, loading: false, end: true } }));
    } catch { set((s) => ({ feed: { ...s.feed, loading: false } })); } finally { loading = false; }
  },
  reload: async () => { page = 1; sourceEnded = false; seenKeys = new Set(); buffer = []; set((s) => ({ feed: { items: [], loading: false, end: false }, statusMap: {} })); await get().loadMore(); },
  resetAll: () => { page = 1; sourceEnded = false; seenKeys = new Set(); buffer = []; set((s) => ({ feed: { items: [], loading: false, end: false }, statusMap: {} })); },
  setStatus: (galleryId, status) => { set((s) => ({ statusMap: { ...s.statusMap, [galleryId]: status } })); },
}));

void (async () => {
  await listen<{ task_id: string; status: string; progress_current: number; progress_total: number }>('task://progress', (event) => { const p = event.payload; const store = useAhentaiBrowseStore.getState(); let found: string | null = null; for (const [key, s] of Object.entries(store.statusMap)) { if (s.taskId === p.task_id) { found = key; break; } } if (found) { useAhentaiBrowseStore.setState((s) => ({ statusMap: { ...s.statusMap, [found!]: { ...s.statusMap[found!], taskStatus: p.status, progressCurrent: p.progress_current, progressTotal: p.progress_total } } })); } });
  await listen<{ book_id: string }>('book://deleted', (event) => { useAhentaiBrowseStore.setState((s) => { const sm = { ...s.statusMap }; let changed = false; for (const [key, st] of Object.entries(sm)) { if (st.localBookId === event.payload.book_id) { sm[key] = { ...st, localBookId: undefined }; changed = true; } } return changed ? { statusMap: sm } : {}; }); });
})();