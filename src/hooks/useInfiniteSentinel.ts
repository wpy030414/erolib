import { useCallback, useEffect, useRef, useState } from 'react';

interface FeedState {
  loading: boolean;
  end: boolean;
}

export interface InfiniteSentinelOptions {
  rootMargin?: string;
  feedState?: FeedState;
}

/** Arms an IntersectionObserver on the element the returned ref callback is
 *  attached to; calls `onLoad` when the sentinel scrolls into view (300px
 *  lookahead by default, so the next page is fetched before the user reaches
 *  the bottom). The observer stays armed across tab/display flips (it reacts
 *  to `display:none` changes), so the caller's `onLoad` must be idempotent —
 *  a no-op while a load is in flight or the feed has ended.
 *
 *  **Auto-fill** (viewport taller than one page of content): the sentinel
 *  stays visible after the initial load and IntersectionObserver only fires
 *  on transitions, so three mechanisms keep the fill loop alive:
 *  1. **feedState watch**: when `loading` flips true → false and the feed has
 *     not ended, re-checks the sentinel after the paint (rAF). This closes
 *     the race where a MutationObserver recheck fires mid-load and gets
 *     swallowed by the store's loading guard.
 *  2. **MutationObserver** on the sentinel's parent container (subtree — new
 *     items land inside the grid, a grandchild of the sentinel's parent):
 *     re-checks whether the sentinel is still visible.
 *  3. **resize listener** on window: re-checks after viewport size changes.
 *
 *  The loop stops naturally once content fills the viewport (sentinel moves
 *  below the fold) or the source runs out (`feedState.end` is true).
 *
 *  The sentinel element may mount late (e.g. conditionally rendered once the
 *  first page loads): the callback ref flips state, re-running the arming
 *  effect — `observe()` is idempotent, so this is safe for always-present
 *  sentinels too. */
export function useInfiniteSentinel(
  onLoad: () => void,
  options?: InfiniteSentinelOptions,
) {
  const [sentinel, setSentinel] = useState<HTMLElement | null>(null);
  const sentinelRef = useCallback((el: HTMLElement | null) => setSentinel(el), []);
  const onLoadRef = useRef(onLoad);
  onLoadRef.current = onLoad;

  const rootMargin = options?.rootMargin ?? '300px';
  const margin = parseFloat(rootMargin) || 0;

  // Latest recheck, callable from the feedState watcher below.
  const recheckRef = useRef<() => void>(() => {});

  useEffect(() => {
    if (!sentinel) return;

    const recheck = () => {
      const rect = sentinel.getBoundingClientRect();
      if (rect.top < window.innerHeight + margin) onLoadRef.current();
    };
    recheckRef.current = recheck;

    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) onLoadRef.current();
      },
      { rootMargin },
    );
    io.observe(sentinel);

    let mo: MutationObserver | null = null;
    const parent = sentinel.parentElement;
    if (parent) {
      mo = new MutationObserver(() => recheck());
      mo.observe(parent, { childList: true, subtree: true });
    }

    const onResize = () => recheck();
    window.addEventListener('resize', onResize);

    // The sentinel may already be visible when armed (late mount after the
    // first page rendered).
    recheck();

    return () => {
      io.disconnect();
      mo?.disconnect();
      window.removeEventListener('resize', onResize);
      recheckRef.current = () => {};
    };
  }, [sentinel, rootMargin, margin]);

  // feedState watch: a finished load (loading true → false) with more to come
  // means the DOM is about to settle with new items — re-check after paint.
  const loading = options?.feedState?.loading ?? false;
  const end = options?.feedState?.end ?? false;
  const prevLoadingRef = useRef(false);
  useEffect(() => {
    if (prevLoadingRef.current && !loading && !end) {
      requestAnimationFrame(() => recheckRef.current());
    }
    prevLoadingRef.current = loading;
  }, [loading, end]);

  return sentinelRef;
}
