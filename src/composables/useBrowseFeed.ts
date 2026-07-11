import { reactive, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getThumb, setThumb } from '@/services/thumb-cache';
import type { CardStatus } from '@/types';
import type { TaskItem } from '@/stores/tasks';

/**
 * Delegate composable that owns the mechanics of a browse feed: pagination
 * (items / loading / end / cursor), the cover-blob map (防盗链 proxy +
 * IndexedDB cache), the per-card local/task status map, and a single
 * `task://progress` listener that flips cards even while their view is
 * unmounted.
 *
 * Source-specific behaviour is injected as functions, so the composable knows
 * nothing about Pixiv vs EHentai:
 *  - `keyOf` / `statusKeyOf` — the status-map key for an item / for a status
 *    row (Pixiv: `work.id` / `s.workId`; EHentai: gallery URL both sides).
 *  - `coverKeyOf` — IndexedDB + coverMap key (Pixiv: `work.id`; EHentai: `gid`
 *    — stable across an EX toggle, unlike the gallery URL). Defaults to keyOf.
 *  - `coverUrlOf` — the remote URL to proxy-fetch (or null for no cover).
 *  - `fetchStatus` — batch local/task state lookup.
 *  - `proxyCover` — proxy-fetch cover bytes (i.pximg.net / e-hentai hotlink).
 *  - `fetchPage(cursor)` — **absorbs all pagination models**: it returns the
 *    items, the next cursor, and whether the feed ended. The caller decides
 *    its own end rule (Pixiv recommend is one-shot, bookmark is offset/total,
 *    following/search are page-based, EHentai is a gid cursor).
 *
 * `shared` lets several feeds (Pixiv's four tabs overlap on the same works)
 * share one coverMap / statusMap / coverLoading, so a cover or download state
 * resolved in one tab is visible in the others. Only the instance with
 * `listen: true` arms the progress listener for a shared group.
 *
 * The returned `feed` is a single reactive `{ items, loading, end }` — bind it
 * straight to <FeedList>. */
export interface BrowseFeedShared<TStatus extends CardStatus> {
  coverMap: Record<string, string | null>;
  statusMap: Record<string, TStatus>;
  coverLoading: Set<string>;
}

export interface UseBrowseFeedOptions<
  TItem,
  TKey extends string,
  TStatus extends CardStatus,
  TCursor,
> {
  keyOf: (item: TItem) => TKey;
  statusKeyOf: (status: TStatus) => TKey;
  coverKeyOf?: (item: TItem) => string;
  coverUrlOf: (item: TItem) => string | null;
  fetchStatus: (keys: TKey[]) => Promise<TStatus[]>;
  fetchPage: (
    cursor: TCursor,
  ) => Promise<{ items: TItem[]; nextCursor: TCursor; end: boolean }>;
  proxyCover: (url: string) => Promise<number[]>;
  initialCursor: TCursor;
  /** Share maps across feeds (Pixiv). Omit for a standalone feed (EHentai). */
  shared?: BrowseFeedShared<TStatus>;
  /** Register the task://progress listener (default true). Set false on the
   *  satellite instances of a shared group so only one listener arms. */
  listen?: boolean;
}

/** Unified page size for browse feeds (Pixiv / EHentai). The composable
 *  buffers source items and tops up across the source's own heterogeneous page
 *  boundaries (Pixiv ~30/~60, EHentai 25, recommend one-shot) so the grid
 *  always grows in fixed 48-item pages regardless of source. Matches the
 *  library grid + the download batch size for a consistent feel; surplus items
 *  past a page boundary stay buffered for the next loadMore. */
const BROWSE_PAGE_SIZE = 48;

const TERMINAL = ['completed', 'failed', 'cancelled'];

export function useBrowseFeed<
  TItem,
  TKey extends string,
  TStatus extends CardStatus,
  TCursor,
>(opts: UseBrowseFeedOptions<TItem, TKey, TStatus, TCursor>) {
  // Maps come from the shared group when given (Pixiv's 4 tabs), else this
  // instance owns a private set (EHentai).
  const coverMap =
    opts.shared?.coverMap ?? reactive<Record<string, string | null>>({});
  const statusMap =
    opts.shared?.statusMap ?? reactive<Record<string, TStatus>>({});
  const coverLoading = opts.shared?.coverLoading ?? new Set<string>();
  const coverKeyOf = opts.coverKeyOf ?? ((item: TItem) => opts.keyOf(item));

  // Cast: Vue's `reactive` rewrites the item array type to `UnwrapNestedRefs`
  // for a generic TItem, which breaks push/forEach. The proxy is the runtime
  // we want; this just restores the plain shape for the type checker.
  const feed = reactive({ items: [] as TItem[], loading: false, end: false }) as {
    items: TItem[];
    loading: boolean;
    end: boolean;
  };
  let cursor: TCursor = opts.initialCursor;
  // Buffer of source items not yet handed to the grid. Source page sizes
  // rarely divide BROWSE_PAGE_SIZE evenly, so each loadMore tops this up to a
  // full page across one or more source fetches; the surplus stays for next.
  let buffer: TItem[] = [];
  // Whether the underlying source feed has reported no more items.
  let sourceEnded = false;

  async function refreshStatus(keys: TKey[]) {
    if (!keys.length) return;
    try {
      const list = await opts.fetchStatus(keys);
      for (const s of list) statusMap[opts.statusKeyOf(s)] = s;
    } catch (e) {
      console.error('refresh browse status:', e);
    }
  }

  async function loadMore() {
    if (feed.loading || feed.end) return;
    feed.loading = true;
    try {
      // Top up the buffer across the source's own page boundaries until it
      // holds at least one unified page, or the source runs out. This flattens
      // Pixiv's ~30/~60 and EHentai's 25 into a steady 48/page.
      while (buffer.length < BROWSE_PAGE_SIZE && !sourceEnded) {
        const res = await opts.fetchPage(cursor);
        if (res.items.length === 0) {
          sourceEnded = true;
          break;
        }
        buffer.push(...res.items);
        cursor = res.nextCursor;
        if (res.end) sourceEnded = true;
      }
      // Hand one page (48, or whatever remains) to the grid.
      const page = buffer.splice(0, BROWSE_PAGE_SIZE);
      if (page.length === 0) {
        feed.end = true;
      } else {
        // Resolve local/task state first so cards render correctly (no flash
        // of the red dot before the download/local state arrives).
        await refreshStatus(page.map(opts.keyOf));
        feed.items.push(...page);
        if (sourceEnded && buffer.length === 0) feed.end = true;
      }
    } catch (e) {
      console.error('load feed:', e);
      // On a hard failure with nothing yet shown, mark ended so the spinner
      // stops; partial loads keep their items and can be retried via reload.
      if (feed.items.length === 0 && buffer.length === 0) feed.end = true;
    } finally {
      feed.loading = false;
    }
  }

  /** Fetch a cover via the backend proxy (the source host hotlink-blocks the
   *  WKWebView). Cached in coverMap as a blob URL; backed by IndexedDB so
   *  repeat covers load instantly across reloads/view-switches. */
  async function loadCover(item: TItem) {
    const key = coverKeyOf(item);
    const url = opts.coverUrlOf(item);
    if (!url || key in coverMap || coverLoading.has(key)) return;
    coverLoading.add(key);
    coverMap[key] = null;
    try {
      let blob = await getThumb(key);
      if (!blob) {
        const bytes = await opts.proxyCover(url);
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

  function setStatus(key: TKey, status: TStatus) {
    statusMap[key] = status;
  }

  /** Patch the card bound to a task id; returns its key (or null). */
  function updateByTaskId(taskId: string, patch: Partial<TStatus>): TKey | null {
    for (const [k, st] of Object.entries(statusMap) as [string, TStatus][]) {
      if (st.taskId === taskId) {
        statusMap[k as TKey] = { ...st, ...patch };
        return k as TKey;
      }
    }
    return null;
  }

  /** Drop this feed's items + cursor + end flag. Does NOT touch statusMap —
   *  stale rows are harmless (overwritten on re-fetch) and, when maps are
   *  shared, pruning by item key would clobber a sibling feed's state. Use
   *  `clearStatusMap` for a full reset (logout / EX-toggle re-key). */
  function resetFeed() {
    feed.items.splice(0, feed.items.length);
    buffer.splice(0, buffer.length);
    cursor = opts.initialCursor;
    sourceEnded = false;
    feed.end = false;
    feed.loading = false;
  }

  /** Clear every status row (shared map wholesale). Used by EHentai reload
   *  (EX toggle re-keys every gallery URL) and by logout. */
  function clearStatusMap() {
    for (const k of Object.keys(statusMap)) {
      delete statusMap[k];
    }
  }

  /** Drop this feed's items + cursor, then re-fetch the first page. */
  async function reload() {
    resetFeed();
    await loadMore();
  }

  // Auto-load covers whenever items stream in.
  watch(
    () => feed.items.length,
    () => {
      feed.items.forEach(loadCover);
    },
  );

  // Track task progress at the feed level so card state keeps updating even
  // while the owning view is unmounted — a download that finishes elsewhere
  // still flips the card. On a terminal status, re-resolve (completed → local
  // book appears).
  const shouldListen = opts.listen ?? true;
  let unlistenProgress: UnlistenFn | undefined;
  if (shouldListen) {
    void listen<TaskItem>('task://progress', (event) => {
      const p = event.payload;
      const key = updateByTaskId(p.id, {
        taskStatus: p.status,
        progressCurrent: p.progress_current,
        progressTotal: p.progress_total,
      } as Partial<TStatus>);
      if (key && TERMINAL.includes(p.status)) void refreshStatus([key]);
    }).then((fn) => {
      unlistenProgress = fn;
    });
  }

  // A book deleted elsewhere (e.g. the library grid) must drop the
  // "downloaded" marker on its browse card → the card reverts to the
  // not-downloaded (red-dot) state instead of offering a dead "Read" entry.
  let unlistenBookDeleted: UnlistenFn | undefined;
  if (shouldListen) {
    void listen<{ bookId: string }>('book://deleted', (event) => {
      const bookId = event.payload.bookId;
      for (const [k, st] of Object.entries(statusMap) as [string, TStatus][]) {
        if (st.localBookId === bookId) {
          statusMap[k as TKey] = { ...st, localBookId: undefined };
        }
      }
    }).then((fn) => {
      unlistenBookDeleted = fn;
    });
  }

  return {
    feed,
    coverMap,
    statusMap,
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
