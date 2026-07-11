import { onMounted, onBeforeUnmount, watch, type Ref } from 'vue';

/** Arms an IntersectionObserver on `sentinelRef` that calls `onLoad` when the
 *  sentinel scrolls into view (300px lookahead by default, so the next page
 *  is fetched before the user reaches the bottom). The observer stays armed
 *  across tab/display flips (it reacts to `display:none` changes), so the
 *  caller's `onLoad` must be idempotent — a no-op while a load is in flight
 *  or the feed has ended.
 *
 *  If the sentinel mounts AFTER setup (e.g. `v-if`'d in once the first page
 *  loads), the `watch` re-observes it when `sentinelRef` resolves. `observe()`
 *  is idempotent, so this is safe for always-present sentinels too.
 *
 *  Component-only (uses onMounted/onBeforeUnmount); call from a component's
 *  setup, not from a Pinia store. */
export function useInfiniteSentinel(
  sentinelRef: Ref<HTMLElement | null>,
  onLoad: () => void,
  options: { rootMargin?: string } = {},
): void {
  let observer: IntersectionObserver | null = null;
  const rootMargin = options.rootMargin ?? '300px';

  onMounted(() => {
    observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) onLoad();
      },
      { rootMargin },
    );
    // The sentinel is usually mounted by the time onMounted runs; guard anyway
    // for v-if'd sentinels — the watch below covers the late-mount case.
    if (sentinelRef.value) observer.observe(sentinelRef.value);
  });

  // Re-observe when the sentinel mounts/remounts after setup (v-if case).
  watch(sentinelRef, (el) => {
    if (observer && el) observer.observe(el);
  });

  onBeforeUnmount(() => {
    observer?.disconnect();
    observer = null;
  });
}
