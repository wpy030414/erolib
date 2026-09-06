import { listen } from '@tauri-apps/api/event';
import { createStore } from 'zustand/vanilla';
import { getThumb, setThumb } from '@/services/thumb-cache';
import type { CardStatus } from '@/types';

/** Page size for every browse grid's infinite scroll. Source pages (~25/~30/
 *  ~60/100 items) are topped up across the source's own page boundaries into
 *  a steady 48/page before hitting the grid. */
export const BROWSE_PAGE_SIZE = 48;

const COVER_MAX_CONCURRENT = 6;
/** Terminal task statuses that trigger a status re-resolve from the backend. */
const TERMINAL_STATUSES = ['completed', 'failed', 'cancelled'];

export interface BrowseFeedShared<TStatus extends CardStatus> {
  coverMap: Record<string, string | null>;
  statusMap: Record<string, TStatus>;
  coverLoading: Set<string>;
}

export interface BrowseFeedOptions<TItem, TKey extends string, TStatus extends CardStatus, TCursor> {
  /** Stable identity of an item — also the statusMap key. */
  keyOf: (item: TItem) => TKey;
  statusKeyOf: (status: TStatus) => TKey;
  /** IndexedDB thumbnail key — decoupled from the identity when the identity
   *  is re-keyed by a preference (EHentai gid survives an EX toggle). */
  coverKeyOf?: (item: TItem) => string;
  coverUrlOf: (item: TItem) => string | null;
  fetchStatus: (keys: TKey[]) => Promise<TStatus[]>;
  /** One source page: nextCursor + end are constructed HERE from the raw
   *  response — the backend never returns cursor envelopes. */
  fetchPage: (cursor: TCursor) => Promise<{ items: TItem[]; nextCursor: TCursor; end: boolean }>;
  proxyCover: (url: string) => Promise<number[]>;
  initialCursor: TCursor;
  /** Share cover/status maps with sibling feeds (Pixiv's four tabs): the same
   *  work routinely appears in several tabs, so a cover or download state
   *  resolved in one must be visible in the others. */
  shared?: BrowseFeedShared<TStatus>;
  /** Arm the app-lifetime task://progress + book://deleted listeners. Arm it
   *  on exactly one feed per source. */
  listen?: boolean;
}

export interface BrowseFeedInst<TItem, TStatus extends CardStatus> {
  feed: { items: TItem[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>;
  statusMap: Record<string, TStatus>;
  /** Latest kernel snapshot (feed + statusVer) — for mirror subscriptions. */
  getState: () => { feed: { items: TItem[]; loading: boolean; end: boolean }; statusVer: number };
  subscribe: (fn: () => void) => () => void;
  loadMore: () => Promise<void>;
  reload: () => Promise<void>;
  resetFeed: () => void;
  clearStatusMap: () => void;
  refreshStatus: (keys: string[]) => Promise<void>;
  setStatus: (key: string, status: TStatus) => void;
  /** Patch the status whose taskId matches; returns its key or null. */
  updateByTaskId: (taskId: string, patch: Partial<TStatus>) => string | null;
  loadCover: (item: TItem) => Promise<void>;
}

/**
 * Vanilla (React-free) browse feed kernel — one instance per source feed.
 * Pagination (seenKeys dedupe + cross-source-page buffer flattened to 48),
 * the cover concurrency gate with IndexedDB caching, and the status map all
 * live here; stores instantiate it with their source-specific fetchPage.
 * Maps are mutable and versioned by `statusVer` bumps so subscribers
 * (mirroring stores) know when to re-render. Listeners live for the app
 * lifetime — instances are module-level singletons, never per-mount.
 */
export function createBrowseFeed<TItem, TKey extends string, TStatus extends CardStatus, TCursor>(
  opts: BrowseFeedOptions<TItem, TKey, TStatus, TCursor>,
): BrowseFeedInst<TItem, TStatus> {
  const keyOf = opts.keyOf;
  const coverKeyOf = opts.coverKeyOf ?? opts.keyOf;

  const ownMaps: BrowseFeedShared<TStatus> = { coverMap: {}, statusMap: {}, coverLoading: new Set() };
  const maps = opts.shared ?? ownMaps;

  const store = createStore<{ feed: { items: TItem[]; loading: boolean; end: boolean }; statusVer: number }>(() => ({
    feed: { items: [], loading: false, end: false },
    statusVer: 0,
  }));
  const bump = () => store.setState((s) => ({ statusVer: s.statusVer + 1 }));

  // Internal mutable pagination state.
  let cursor: TCursor = opts.initialCursor;
  let seenKeys = new Set<TKey>();
  let buffer: TItem[] = [];
  let sourceEnded = false;
  let loadingRef = false;

  // Cover concurrency gate.
  let inFlight = 0;
  const gateQueue: Array<() => void> = [];
  function gateEnter(): Promise<void> {
    if (inFlight < COVER_MAX_CONCURRENT) { inFlight++; return Promise.resolve(); }
    return new Promise((resolve) => { gateQueue.push(() => { inFlight++; resolve(); }); });
  }
  function gateLeave() { inFlight--; const next = gateQueue.shift(); if (next) next(); }

  async function loadCover(item: TItem) {
    const key = coverKeyOf(item);
    if (maps.coverMap[key] !== undefined) return;
    const url = opts.coverUrlOf(item);
    if (!url) { maps.coverMap[key] = null; bump(); return; }
    maps.coverLoading.add(key);
    maps.coverMap[key] = null;
    try {
      await gateEnter();
      let blob = await getThumb(key);
      if (!blob) {
        const bytes = await opts.proxyCover(url);
        blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
        void setThumb(key, blob);
      }
      if (maps.coverLoading.has(key)) {
        const objUrl = URL.createObjectURL(blob);
        const old = maps.coverMap[key];
        if (old) URL.revokeObjectURL(old);
        maps.coverMap[key] = objUrl;
        maps.coverLoading.delete(key);
        bump();
      }
    } catch {
      maps.coverMap[key] = null;
      maps.coverLoading.delete(key);
      bump();
    } finally {
      gateLeave();
    }
  }

  async function refreshStatus(keys: string[]) {
    if (keys.length === 0) return;
    try {
      const statuses = await opts.fetchStatus(keys as TKey[]);
      for (const s of statuses) maps.statusMap[opts.statusKeyOf(s)] = s;
      bump();
    } catch { /* keep previous statuses */ }
  }

  function setStatus(key: string, status: TStatus) {
    maps.statusMap[key] = status;
    bump();
  }

  function updateByTaskId(taskId: string, patch: Partial<TStatus>): string | null {
    for (const [key, s] of Object.entries(maps.statusMap)) {
      if (s.taskId === taskId) {
        maps.statusMap[key] = { ...s, ...patch };
        bump();
        return key;
      }
    }
    return null;
  }

  function clearStatusMap() {
    for (const key of Object.keys(maps.statusMap)) delete maps.statusMap[key];
    bump();
  }

  async function loadMore() {
    if (loadingRef || sourceEnded) return;
    loadingRef = true;
    store.setState((s) => ({ feed: { ...s.feed, loading: true } }));
    try {
      // Top up the buffer across the source's own page boundaries until it
      // holds at least one unified page, or the source runs out. seenKeys
      // persists across loadMore() calls (cleared only on resetFeed) so items
      // left in the buffer stay tracked and server-side shifts of old content
      // into the next page are silently dropped. An empty source page also
      // terminates the feed.
      while (buffer.length < BROWSE_PAGE_SIZE && !sourceEnded) {
        const res = await opts.fetchPage(cursor);
        if (res.items.length === 0) { sourceEnded = true; break; }
        for (const item of res.items) {
          const key = keyOf(item);
          if (!seenKeys.has(key)) { seenKeys.add(key); buffer.push(item); }
        }
        cursor = res.nextCursor;
        if (res.end) sourceEnded = true;
      }
      // Hand one page (48, or whatever remains) to the grid.
      const page = buffer.splice(0, BROWSE_PAGE_SIZE);
      if (page.length === 0) {
        store.setState((s) => ({ feed: { ...s.feed, loading: false, end: true } }));
      } else {
        // Resolve local/task state BEFORE appending so cards render correctly
        // (no flash of the red dot before the download/local state arrives).
        await refreshStatus(page.map(keyOf));
        store.setState((s) => ({ feed: { items: [...s.feed.items, ...page], loading: false, end: sourceEnded && buffer.length === 0 } }));
        for (const item of page) void loadCover(item);
      }
    } catch (e) {
      console.error('load feed:', e);
      // On a hard failure with nothing yet shown, mark ended so the spinner
      // stops; partial loads keep their items and can be retried via reload.
      store.setState((s) => ({ feed: { ...s.feed, loading: false, end: s.feed.items.length === 0 && buffer.length === 0 } }));
    } finally {
      loadingRef = false;
    }
  }

  function clearPagination() {
    store.setState({ feed: { items: [], loading: false, end: false } });
    seenKeys = new Set();
    buffer = [];
    cursor = opts.initialCursor;
    sourceEnded = false;
    loadingRef = false;
  }

  async function reload() {
    clearPagination();
    await loadMore();
  }

  function resetFeed() {
    clearPagination();
  }

  if (opts.listen) {
    // App-lifetime listeners (instances are module-level singletons).
    void (async () => {
      await listen<{ id: string; status: string; progress_current: number; progress_total: number }>('task://progress', (event) => {
        const p = event.payload;
        const key = updateByTaskId(p.id, {
          taskStatus: p.status,
          progressCurrent: p.progress_current,
          progressTotal: p.progress_total,
        } as Partial<TStatus>);
        // On terminal status, re-resolve from the backend so the card flips
        // to its real local/task state (e.g. gains localBookId when done).
        if (key && TERMINAL_STATUSES.includes(p.status)) void refreshStatus([key]);
      });
      await listen<{ bookId: string }>('book://deleted', (event) => {
        for (const [key, s] of Object.entries(maps.statusMap)) {
          if (s.localBookId === event.payload.bookId) {
            maps.statusMap[key] = { ...s, localBookId: undefined };
            bump();
          }
        }
      });
    })();
  }

  return {
    feed: store.getState().feed,
    coverMap: maps.coverMap,
    statusMap: maps.statusMap,
    getState: () => store.getState(),
    subscribe: (fn: () => void) => store.subscribe(fn),
    loadMore,
    reload,
    resetFeed,
    clearStatusMap,
    refreshStatus,
    setStatus,
    updateByTaskId,
    loadCover,
  };
}
