import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getThumb, setThumb } from '@/services/thumb-cache';
import { api } from '@/services/api';
import type { NicecatComicItem, NicecatBrowseStatus } from '@/types';

const BROWSE_PAGE_SIZE = 48; const PAGE_HINT = 60;
let searchCursor: string | null = null; let searchEnded = false; let searchLoading = false;
let seenKeys = new Set<string>(); let buffer: NicecatComicItem[] = [];
let coverLoading = new Set<string>(); let inFlight = 0;
const gateQueue: Array<() => void> = [];

function gateEnter(): Promise<void> { if (inFlight < 6) { inFlight++; return Promise.resolve(); } return new Promise((r) => { gateQueue.push(() => { inFlight++; r(); }); }); }
function gateLeave() { inFlight--; const n = gateQueue.shift(); if (n) n(); }

async function loadCover(uid: string, imageUrl: string | null, coverMap: Record<string, string | null>) {
  if (coverMap[uid] !== undefined || !imageUrl) { if (!imageUrl) coverMap[uid] = null; return; }
  coverLoading.add(uid); coverMap[uid] = null;
  try { await gateEnter(); let blob = await getThumb(uid); if (!blob) { const bytes = await api.nicecatProxyThumb(imageUrl); blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }); void setThumb(uid, blob); } if (coverLoading.has(uid)) { const old = coverMap[uid]; if (old) URL.revokeObjectURL(old); coverMap[uid] = URL.createObjectURL(blob); coverLoading.delete(uid); } } catch { coverMap[uid] = null; coverLoading.delete(uid); } finally { gateLeave(); }
}

interface NicecatBrowseState {
  keyword: string; sections: { name: string; comics: NicecatComicItem[] }[];
  cachedSections: { name: string; comics: NicecatComicItem[] }[];
  homeLoading: boolean; homeError: string | null;
  feed: { items: NicecatComicItem[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>; statusMap: Record<string, NicecatBrowseStatus>;
  isSearching: boolean; loaded: boolean;
  loadHomepage: () => Promise<void>; reload: (force?: boolean) => Promise<void>; resetAll: () => void;
  setStatus: (comicId: string, status: NicecatBrowseStatus) => void; isBusy: (comicId: string) => boolean;
}

export const useNicecatBrowseStore = create<NicecatBrowseState>((set, get) => ({
  keyword: '', sections: [], cachedSections: [], homeLoading: false, homeError: null,
  feed: { items: [], loading: false, end: false }, coverMap: {}, statusMap: {}, isSearching: false, loaded: false,

  loadHomepage: async () => {
    set({ homeLoading: true, homeError: null });
    try {
      const raw = await api.nicecatFetchApi('/api/HomeFeed/randomFeed', {}) as any;
      const sections: { name: string; comics: NicecatComicItem[] }[] = [];
      if (Array.isArray(raw)) { for (const sec of raw) { if (sec.name && Array.isArray(sec.comics)) sections.push(sec); } }
      set((s) => { for (const sec of sections) { for (const comic of sec.comics) void loadCover(comic.uid, comic.image, s.coverMap); } return { sections, cachedSections: sections, homeLoading: false, loaded: true }; });
      const allUids = sections.flatMap((s) => s.comics.map((c) => c.uid));
      if (allUids.length > 0) { void api.nicecatBrowseStatus(allUids).then((statuses) => { set((s) => { const sm = { ...s.statusMap }; for (const st of statuses) sm[st.comicId] = st; return { statusMap: sm }; }); }).catch(() => {}); }
    } catch (e) { set({ homeError: String(e), homeLoading: false }); }
  },

  reload: async (force) => {
    const { isSearching, cachedSections, keyword } = get();
    if (isSearching) {
      set((s) => ({ cachedSections: s.sections }));
      searchCursor = null; searchEnded = false; seenKeys = new Set(); buffer = [];
      set((s) => ({ feed: { items: [], loading: false, end: false } }));
      searchLoading = true; set((s) => ({ feed: { ...s.feed, loading: true } }));
      try {
        while (buffer.length < BROWSE_PAGE_SIZE && !searchEnded) {
          const result = await api.nicecatFetchApi('/api/Search/ComicSearch', { keyword, searchId: searchCursor ?? '' }) as any;
          const items = Array.isArray(result.items) ? result.items : [];
          for (const item of items) { if (!seenKeys.has(item.uid)) { seenKeys.add(item.uid); buffer.push(item); } }
          const nextCursor = result.nextCursor || ''; if (!nextCursor || items.length < PAGE_HINT) { searchEnded = true; break; }
          searchCursor = nextCursor;
        }
        const pageItems = buffer.splice(0, BROWSE_PAGE_SIZE);
        set((s) => { const newItems = [...s.feed.items, ...pageItems]; for (const item of pageItems) void loadCover(item.uid, item.image, s.coverMap); return { feed: { items: newItems, loading: false, end: searchEnded && buffer.length === 0 } }; });
        void api.nicecatBrowseStatus(pageItems.map((i) => i.uid)).then((statuses) => { set((s) => { const sm = { ...s.statusMap }; for (const st of statuses) sm[st.comicId] = st; return { statusMap: sm }; }); }).catch(() => {});
      } catch { set((s) => ({ feed: { ...s.feed, loading: false } })); } finally { searchLoading = false; }
    } else {
      if (cachedSections.length > 0 && !force) set({ sections: cachedSections });
      else await get().loadHomepage();
    }
  },

  resetAll: () => { searchCursor = null; searchEnded = false; seenKeys = new Set(); buffer = []; set({ sections: [], cachedSections: [], feed: { items: [], loading: false, end: false }, statusMap: {}, homeLoading: false, homeError: null, loaded: false }); },
  setStatus: (comicId, status) => { set((s) => ({ statusMap: { ...s.statusMap, [comicId]: status } })); },
  isBusy: (comicId) => { const st = get().statusMap[comicId]; if (!st?.taskStatus) return false; return ['pending', 'running', 'paused'].includes(st.taskStatus); },
}));

void (async () => {
  await listen<{ task_id: string; status: string; progress_current: number; progress_total: number }>('task://progress', (event) => { const p = event.payload; const store = useNicecatBrowseStore.getState(); let found: string | null = null; for (const [key, s] of Object.entries(store.statusMap)) { if (s.taskId === p.task_id) { found = key; break; } } if (found) { useNicecatBrowseStore.setState((s) => ({ statusMap: { ...s.statusMap, [found!]: { ...s.statusMap[found!], taskStatus: p.status, progressCurrent: p.progress_current, progressTotal: p.progress_total } } })); } });
  await listen<{ book_id: string }>('book://deleted', (event) => { useNicecatBrowseStore.setState((s) => { const sm = { ...s.statusMap }; let changed = false; for (const [key, st] of Object.entries(sm)) { if (st.localBookId === event.payload.book_id) { sm[key] = { ...st, localBookId: undefined }; changed = true; } } return changed ? { statusMap: sm } : {}; }); });
})();