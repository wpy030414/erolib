import { create } from 'zustand';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getThumb, setThumb } from '@/services/thumb-cache';
import { api } from '@/services/api';
import type { PixivWork, PixivBrowseStatus } from '@/types';

const BROWSE_PAGE_SIZE = 48; const SOURCE_END_HINT = 30;
const pages = { recommend: 1, following: 1, bookmark: 0, search: 1 };
const cursors = { recommend: 1, following: 1, bookmark: 0, search: 1 };
const sourceEnded = { recommend: false, following: false, bookmark: false, search: false };
const loading = { recommend: false, following: false, bookmark: false, search: false };
let seenKeys = new Set<string>(); let buffer: PixivWork[] = [];
let coverLoading = new Set<string>(); let inFlight = 0;
const gateQueue: Array<() => void> = [];

function gateEnter(): Promise<void> { if (inFlight < 6) { inFlight++; return Promise.resolve(); } return new Promise((r) => { gateQueue.push(() => { inFlight++; r(); }); }); }
function gateLeave() { inFlight--; const n = gateQueue.shift(); if (n) n(); }

async function loadCover(workId: string, coverUrl: string | null, coverMap: Record<string, string | null>) {
  if (coverMap[workId] !== undefined || !coverUrl) { if (!coverUrl) coverMap[workId] = null; return; }
  coverLoading.add(workId); coverMap[workId] = null;
  try { await gateEnter(); let blob = await getThumb(workId); if (!blob) { const bytes = await api.pixivProxyImage(coverUrl); blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' }); void setThumb(workId, blob); } if (coverLoading.has(workId)) { const old = coverMap[workId]; if (old) URL.revokeObjectURL(old); coverMap[workId] = URL.createObjectURL(blob); coverLoading.delete(workId); } } catch { coverMap[workId] = null; coverLoading.delete(workId); } finally { gateLeave(); }
}

async function fetchPageForTarget(target: string, state: any): Promise<{ items: PixivWork[]; end: boolean }> {
  switch (target) {
    case 'recommend': { const items = await api.listPixivRecommended(1); return { items, end: true }; }
    case 'following': { const items = await api.listPixivFollowingFeed(pages.following); pages.following++; return { items, end: items.length < SOURCE_END_HINT }; }
    case 'bookmark': { const offset = cursors.bookmark; const result = await api.listPixivBookmarks(offset, 100); cursors.bookmark = result.nextCursor ?? result.total; return { items: result.items, end: cursors.bookmark >= result.total }; }
    case 'search': { if (!state.searchKeyword) return { items: [], end: true }; const items = await api.searchPixivIllusts(state.searchKeyword, pages.search); pages.search++; return { items, end: items.length < SOURCE_END_HINT }; }
    default: return { items: [], end: true };
  }
}

interface PixivBrowseState {
  searchKeyword: string;
  recommend: { items: PixivWork[]; loading: boolean; end: boolean };
  following: { items: PixivWork[]; loading: boolean; end: boolean };
  bookmark: { items: PixivWork[]; loading: boolean; end: boolean };
  search: { items: PixivWork[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>; statusMap: Record<string, PixivBrowseStatus>;
  loadMore: (target: string) => Promise<void>; reload: (target: string) => Promise<void>;
  setStatus: (workId: string, status: PixivBrowseStatus) => void; setSearchKeyword: (kw: string) => void; resetAll: () => void;
}

export const usePixivBrowseStore = create<PixivBrowseState>((set, get) => ({
  searchKeyword: '',
  recommend: { items: [], loading: false, end: false }, following: { items: [], loading: false, end: false },
  bookmark: { items: [], loading: false, end: false }, search: { items: [], loading: false, end: false },
  coverMap: {}, statusMap: {},

  loadMore: async (target) => {
    if (loading[target as keyof typeof loading] || sourceEnded[target as keyof typeof sourceEnded]) return;
	    loading[target as keyof typeof loading] = true;
    set((s) => ({ [target]: { ...s[target as keyof typeof s], loading: true } } as any));
    try {
      while (buffer.length < BROWSE_PAGE_SIZE && !sourceEnded[target as keyof typeof sourceEnded]) { const result = await fetchPageForTarget(target, get()); for (const item of result.items) { if (!seenKeys.has(item.id)) { seenKeys.add(item.id); buffer.push(item); } } if (result.end) { sourceEnded[target as keyof typeof sourceEnded] = true; break; } }
      const pageItems = buffer.splice(0, BROWSE_PAGE_SIZE);
      if (pageItems.length > 0) { set((s) => { const newItems = [...(s[target as keyof typeof s] as any).items, ...pageItems]; for (const item of pageItems) void loadCover(item.id, item.coverUrl ?? null, s.coverMap); return { [target]: { items: newItems, loading: false, end: sourceEnded[target as keyof typeof sourceEnded] && buffer.length === 0 } } as any; }); void api.pixivBrowseStatus(pageItems.map((i) => i.id)).then((statuses) => { set((s) => { const sm = { ...s.statusMap }; for (const st of statuses) sm[st.workId] = st; return { statusMap: sm }; }); }).catch(() => {}); }
      else set((s) => ({ [target]: { ...s[target as keyof typeof s], loading: false, end: true } } as any));
    } catch { set((s) => ({ [target]: { ...s[target as keyof typeof s], loading: false } } as any)); } finally { loading[target as keyof typeof loading] = false; }
  },
  reload: async (target) => { pages[target as keyof typeof pages] = target === 'bookmark' ? 0 : 1; cursors[target as keyof typeof cursors] = target === 'bookmark' ? 0 : 1; sourceEnded[target as keyof typeof sourceEnded] = false; seenKeys = new Set(); buffer = []; set((s) => ({ [target]: { items: [], loading: false, end: false }, statusMap: {} } as any)); await get().loadMore(target); },
  setStatus: (workId, status) => { set((s) => ({ statusMap: { ...s.statusMap, [workId]: status } })); },
  setSearchKeyword: (kw) => { const trimmed = kw.trim(); set({ searchKeyword: trimmed }); if (trimmed) { pages.search = 1; sourceEnded.search = false; set((s) => ({ search: { items: [], loading: false, end: false } })); void get().loadMore('search'); } },
  resetAll: () => { pages.recommend = 1; pages.following = 1; pages.bookmark = 0; pages.search = 1; cursors.recommend = 1; cursors.following = 1; cursors.bookmark = 0; cursors.search = 1; sourceEnded.recommend = false; sourceEnded.following = false; sourceEnded.bookmark = false; sourceEnded.search = false; seenKeys = new Set(); buffer = []; set({ recommend: { items: [], loading: false, end: false }, following: { items: [], loading: false, end: false }, bookmark: { items: [], loading: false, end: false }, search: { items: [], loading: false, end: false }, statusMap: {} }); },
}));

void (async () => {
  await listen<{ task_id: string; status: string; progress_current: number; progress_total: number }>('task://progress', (event) => { const p = event.payload; const store = usePixivBrowseStore.getState(); let found: string | null = null; for (const [key, s] of Object.entries(store.statusMap)) { if (s.taskId === p.task_id) { found = key; break; } } if (found) { usePixivBrowseStore.setState((s) => ({ statusMap: { ...s.statusMap, [found!]: { ...s.statusMap[found!], taskStatus: p.status, progressCurrent: p.progress_current, progressTotal: p.progress_total } } })); } });
  await listen<{ book_id: string }>('book://deleted', (event) => { usePixivBrowseStore.setState((s) => { const sm = { ...s.statusMap }; let changed = false; for (const [key, st] of Object.entries(sm)) { if (st.localBookId === event.payload.book_id) { sm[key] = { ...st, localBookId: undefined }; changed = true; } } return changed ? { statusMap: sm } : {}; }); });
})();