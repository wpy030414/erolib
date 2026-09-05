import { useState, useEffect, useRef, useCallback } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getThumb, setThumb } from '@/services/thumb-cache';
import type { CardStatus } from '@/types';

export const BROWSE_PAGE_SIZE = 48;
const COVER_MAX_CONCURRENT = 6;

export interface BrowseFeedShared<TStatus extends CardStatus> {
  coverMap: Record<string, string | null>;
  statusMap: Record<string, TStatus>;
  coverLoading: Set<string>;
}

export interface UseBrowseFeedOptions<TItem, TKey, TStatus extends CardStatus, TCursor> {
  keyOf: (item: TItem) => TKey;
  statusKeyOf: (status: TStatus) => TKey;
  coverKeyOf?: (item: TItem) => string;
  coverUrlOf: (item: TItem) => string | null;
  fetchStatus: (keys: TKey[]) => Promise<TStatus[]>;
  fetchPage: (cursor: TCursor) => Promise<{ items: TItem[]; nextCursor: TCursor; end: boolean }>;
  proxyCover: (url: string) => Promise<number[]>;
  initialCursor: TCursor;
  shared?: BrowseFeedShared<TStatus>;
  listen?: boolean;
}

export interface BrowseFeedReturn<TItem, TStatus extends CardStatus> {
  feed: { items: TItem[]; loading: boolean; end: boolean };
  coverMap: Record<string, string | null>;
  statusMap: Record<string, TStatus>;
  loadMore: () => Promise<void>;
  reload: () => Promise<void>;
  resetFeed: () => void;
  clearStatusMap: () => void;
  refreshStatus: (keys: string[]) => Promise<void>;
  setStatus: (key: string, status: TStatus) => void;
  updateByTaskId: (taskId: string, patch: Partial<TStatus>) => string | null;
  loadCover: (item: TItem) => Promise<void>;
}

export function useBrowseFeed<TItem, TKey extends string, TStatus extends CardStatus, TCursor>(
  options: UseBrowseFeedOptions<TItem, TKey, TStatus, TCursor>,
): BrowseFeedReturn<TItem, TStatus> {
  const {
    keyOf, statusKeyOf, coverKeyOf, coverUrlOf,
    fetchStatus, fetchPage, proxyCover, initialCursor,
    shared, listen: shouldListen,
  } = options;

  const coverKey = coverKeyOf ?? keyOf;

  // Shared or local maps
  const ownCoverMap = useRef<Record<string, string | null>>({});
  const ownStatusMap = useRef<Record<string, TStatus>>({});
  const ownCoverLoading = useRef<Set<string>>(new Set());

  const coverMap = shared?.coverMap ?? ownCoverMap.current;
  const statusMap = shared?.statusMap ?? ownStatusMap.current;
  const coverLoading = shared?.coverLoading ?? ownCoverLoading.current;

  // Feed state
  const [items, setItems] = useState<TItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [end, setEnd] = useState(false);
  const [statusVer, setStatusVer] = useState(0); // bump to trigger re-render

  // Internal refs (don't trigger re-render)
  const seenKeys = useRef<Set<TKey>>(new Set());
  const buffer = useRef<TItem[]>([]);
  const cursor = useRef<TCursor>(initialCursor);
  const sourceEnded = useRef(false);
  const loadingRef = useRef(false);
  const gateQueue = useRef<Array<() => void>>([]);
  const inFlight = useRef(0);

  const feed = { items, loading, end };

  // ── Cover concurrency gate ──────────────────────────────────────────

  function gateEnter(): Promise<void> {
    if (inFlight.current < COVER_MAX_CONCURRENT) {
      inFlight.current++;
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      gateQueue.current.push(() => {
        inFlight.current++;
        resolve();
      });
    });
  }

  function gateLeave() {
    inFlight.current--;
    const next = gateQueue.current.shift();
    if (next) next();
  }

  // ── Cover loading ───────────────────────────────────────────────────

  const loadCover = useCallback(async (item: TItem) => {
    const key = coverKey(item);
    if (coverMap[key] !== undefined) return;
    const url = coverUrlOf(item);
    if (!url) {
      coverMap[key] = null;
      return;
    }

    // Mark as loading
    coverLoading.add(key);
    coverMap[key] = null;

    try {
      await gateEnter();
      // Check IndexedDB first
      let blob = await getThumb(key);
      if (!blob) {
        const bytes = await proxyCover(url);
        blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' });
        void setThumb(key, blob);
      }
      if (coverLoading.has(key)) {
        const objUrl = URL.createObjectURL(blob);
        // Revoke old URL if exists
        if (coverMap[key] && coverMap[key] !== null) {
          URL.revokeObjectURL(coverMap[key]!);
        }
        coverMap[key] = objUrl;
        coverLoading.delete(key);
        setStatusVer((v) => v + 1); // trigger re-render
      }
    } catch {
      coverMap[key] = null;
      coverLoading.delete(key);
    } finally {
      gateLeave();
    }
  }, [coverKey, coverUrlOf, proxyCover, coverMap, coverLoading]);

  // ── Status management ───────────────────────────────────────────────

  const refreshStatus = useCallback(async (keys: string[]) => {
    if (keys.length === 0) return;
    try {
      const statuses = await fetchStatus(keys as TKey[]);
      for (const s of statuses) {
        const k = statusKeyOf(s);
        statusMap[k] = s;
      }
      setStatusVer((v) => v + 1);
    } catch { /* ignore */ }
  }, [fetchStatus, statusKeyOf, statusMap]);

  const setStatusFn = useCallback((key: string, status: TStatus) => {
    statusMap[key] = status;
    setStatusVer((v) => v + 1);
  }, [statusMap]);

  const updateByTaskId = useCallback((taskId: string, patch: Partial<TStatus>): string | null => {
    for (const [key, s] of Object.entries(statusMap)) {
      if (s.taskId === taskId) {
        statusMap[key] = { ...s, ...patch };
        setStatusVer((v) => v + 1);
        return key;
      }
    }
    return null;
  }, [statusMap]);

  const clearStatusMap = useCallback(() => {
    for (const key of Object.keys(statusMap)) {
      delete statusMap[key];
    }
    setStatusVer((v) => v + 1);
  }, [statusMap]);

  // ── Pagination ──────────────────────────────────────────────────────

  const loadMore = useCallback(async () => {
    if (loadingRef.current || sourceEnded.current) return;
    loadingRef.current = true;
    setLoading(true);

    try {
      // Keep fetching from source until buffer has 48 items or source ends
      while (buffer.current.length < BROWSE_PAGE_SIZE && !sourceEnded.current) {
        const page = await fetchPage(cursor.current);
        for (const item of page.items) {
          const k = keyOf(item);
          if (!seenKeys.current.has(k)) {
            seenKeys.current.add(k);
            buffer.current.push(item);
          }
        }
        cursor.current = page.nextCursor;
        if (page.end) {
          sourceEnded.current = true;
          break;
        }
      }

      // Emit one page (48 items) to the grid
      const pageItems = buffer.current.splice(0, BROWSE_PAGE_SIZE);
      if (pageItems.length > 0) {
        setItems((prev) => [...prev, ...pageItems]);
        // Load covers for new items
        for (const item of pageItems) {
          void loadCover(item);
        }
      }

      if (sourceEnded.current && buffer.current.length === 0) {
        setEnd(true);
      }
    } catch { /* ignore */ } finally {
      loadingRef.current = false;
      setLoading(false);
    }
  }, [fetchPage, keyOf, loadCover, cursor, sourceEnded, buffer, seenKeys]);

  const reload = useCallback(async () => {
    // Reset
    setItems([]);
    setEnd(false);
    seenKeys.current = new Set();
    buffer.current = [];
    cursor.current = initialCursor;
    sourceEnded.current = false;
    loadingRef.current = false;
    // Fetch first page
    await loadMore();
  }, [loadMore, initialCursor]);

  const resetFeed = useCallback(() => {
    setItems([]);
    setEnd(false);
    seenKeys.current = new Set();
    buffer.current = [];
    cursor.current = initialCursor;
    sourceEnded.current = false;
    loadingRef.current = false;
  }, [initialCursor]);

  // ── Tauri event listeners ───────────────────────────────────────────

  useEffect(() => {
    if (!shouldListen) return;
    const cleanups: UnlistenFn[] = [];

    void (async () => {
      const u1 = await listen<{ task_id: string; status: string; progress_current: number; progress_total: number }>(
        'task://progress',
        (event) => {
          const p = event.payload;
          const key = updateByTaskId(p.task_id, {
            taskStatus: p.status,
            progressCurrent: p.progress_current,
            progressTotal: p.progress_total,
          } as Partial<TStatus>);
          // On terminal status, re-resolve from backend
          if (key && ['completed', 'failed', 'cancelled'].includes(p.status)) {
            void refreshStatus([key]);
          }
        },
      );
      cleanups.push(u1);

      const u2 = await listen<{ book_id: string }>('book://deleted', (event) => {
        for (const [key, s] of Object.entries(statusMap)) {
          if (s.localBookId === event.payload.book_id) {
            statusMap[key] = { ...s, localBookId: undefined };
            setStatusVer((v) => v + 1);
          }
        }
      });
      cleanups.push(u2);
    })();

    return () => {
      cleanups.forEach((fn) => fn());
    };
  }, [shouldListen, updateByTaskId, refreshStatus, statusMap]);

  // ── Load covers for new items on mount ──────────────────────────────

  useEffect(() => {
    for (const item of items) {
      void loadCover(item);
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return {
    feed,
    coverMap,
    get statusMap() { return statusMap as Record<string, TStatus>; },
    loadMore,
    reload,
    resetFeed,
    clearStatusMap,
    refreshStatus,
    setStatus: setStatusFn,
    updateByTaskId,
    loadCover,
  };
}