import { watch, onMounted, onBeforeUnmount, type Ref } from 'vue';

interface FeedState {
  loading: boolean;
  end: boolean;
}

/** Arms an IntersectionObserver on `sentinelRef` that calls `onLoad` when the
 *  sentinel scrolls into view (300px lookahead by default, so the next page
 *  is fetched before the user reaches the bottom). The observer stays armed
 *  across tab/display flips (it reacts to `display:none` changes), so the
 *  caller's `onLoad` must be idempotent — a no-op while a load is in flight
 *  or the feed has ended.
 *
 *  **Auto-fill**: when the viewport is taller than one page of content, the
 *  sentinel stays visible after the initial load — and IntersectionObserver
 *  only fires on *transitions* (in→out / out→in), so it never re-triggers on
 *  its own. Three mechanisms keep the fill loop alive:
 *  1. **feedState watch**: when `loading` flips true → false and the feed has
 *     not ended, re-checks the sentinel after DOM update (nextTick). This is
 *     the key fix — it catches the race where a MutationObserver recheck fires
 *     mid-load and gets swallowed by the store's `loading` guard.
 *  2. **MutationObserver** on the sentinel's container: fires when new items
 *     are rendered into the grid, re-checks whether the sentinel is still
 *     visible. Acts as a fast-path that often avoids needing the feedState
 *     watch round-trip.
 *  3. **resize listener** on window: re-checks after viewport size changes.
 *
 *  The loop stops naturally once content fills the viewport (sentinel moves
 *  below the fold) or the source runs out (`feedState.end` is true).
 *
 *  If the sentinel mounts AFTER setup (e.g. `v-if`'d in once the first page
 *  loads), the `watch` arms IO + MO + rechecks immediately. `observe()` is
 *  idempotent, so this is safe for always-present sentinels too.
 *
 *  Component-only (uses onMounted/onBeforeUnmount); call from a component's
 *  setup, not from a Pinia store. */
export function useInfiniteSentinel(
  sentinelRef: Ref<HTMLElement | null>,
  onLoad: () => void,
  options: { rootMargin?: string; feedState?: FeedState } = {},
): void {
  let io: IntersectionObserver | null = null;
  let mo: MutationObserver | null = null;
  const rootMargin = options.rootMargin ?? '300px';

  /** Returns true when the sentinel sits within the visible viewport (plus
   *  the configured rootMargin lookahead). Used by the MutationObserver and
   *  resize paths to decide whether another page is needed. */
  function isSentinelInViewport(): boolean {
    const el = sentinelRef.value;
    if (!el) return false;
    const rect = el.getBoundingClientRect();
    const margin = parseFloat(rootMargin) || 0;
    return rect.top < window.innerHeight + margin;
  }

  function recheck(): void {
    if (isSentinelInViewport()) onLoad();
  }

  /** Arm the MutationObserver on the sentinel's parent container. Called both
   *  from onMounted (sentinel already present) and the watch (late mount via
   *  v-if). Idempotent — disconnects any prior MO first. */
  function armMutationObserver(el: HTMLElement): void {
    mo?.disconnect();
    const container = el.parentElement;
    if (!container) return;
    // subtree:true is necessary because new items land inside the .md3-grid
    // (a grandchild of the sentinel's parent), not as direct siblings of the
    // sentinel. The callback is lightweight (a getBoundingClientRect check),
    // so per-card mutations during a page render are harmless.
    mo = new MutationObserver(() => recheck());
    mo.observe(container, { childList: true, subtree: true });
  }

  // Core mechanism: when a load finishes (loading true → false) and the feed
  // has not ended, the DOM is about to settle with new items. Use nextTick to
  // let Vue's reactivity flush the DOM patch, then check if the sentinel is
  // still visible — if so, the viewport has room for another page.
  //
  // This closes the race where the MutationObserver fires mid-load and the
  // store's `loading` guard swallows the recheck: by the time this watcher
  // runs, loading has already flipped to false, so the next loadMore() call
  // goes through cleanly.
  if (options.feedState) {
    watch(
      () => options.feedState!.loading,
      (loading, wasLoading) => {
        if (wasLoading && !loading && !options.feedState!.end) {
          // nextTick: Vue has updated the reactive model but the DOM patch is
          // queued. requestAnimationFrame waits for the paint, then recheck
          // sees the actual post-render sentinel position.
          requestAnimationFrame(recheck);
        }
      },
    );
  }

  onMounted(() => {
    io = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) onLoad();
      },
      { rootMargin },
    );
    // The sentinel is usually mounted by the time onMounted runs; guard anyway
    // for v-if'd sentinels — the watch below covers the late-mount case.
    if (sentinelRef.value) {
      io.observe(sentinelRef.value);
      armMutationObserver(sentinelRef.value);
      recheck();
    }

    // Resize listener covers viewport size changes (e.g. window resize,
    // sidebar toggle). Cheap guard around recheck().
    window.addEventListener('resize', recheck);
  });

  // Re-observe when the sentinel mounts/remounts after setup (v-if case).
  // Also immediately check viewport — the sentinel may already be visible
  // (e.g. large viewport + late sentinel mount after first page render).
  watch(sentinelRef, (el) => {
    if (io && el) {
      io.observe(el);
      armMutationObserver(el);
      recheck();
    }
  });

  onBeforeUnmount(() => {
    io?.disconnect();
    io = null;
    mo?.disconnect();
    mo = null;
    window.removeEventListener('resize', recheck);
  });
}
