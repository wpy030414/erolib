import { useEffect, useRef } from 'react';

interface FeedState {
  loading: boolean;
  end: boolean;
}

export function useInfiniteSentinel(
  sentinelRef: React.RefObject<HTMLElement | null>,
  onLoad: () => void,
  options?: {
    rootMargin?: string;
    feedState?: FeedState;
  },
) {
  const onLoadRef = useRef(onLoad);
  onLoadRef.current = onLoad;
  const feedStateRef = useRef(options?.feedState);
  feedStateRef.current = options?.feedState;

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;

    let io: IntersectionObserver | null = null;
    let mo: MutationObserver | null = null;

    const checkSentinel = () => {
      if (!sentinel || !io) return;
      const rect = sentinel.getBoundingClientRect();
      const viewHeight = window.innerHeight;
      const threshold = 300;
      if (rect.top < viewHeight + threshold) {
        onLoadRef.current();
      }
    };

    io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            onLoadRef.current();
          }
        }
      },
      { rootMargin: options?.rootMargin ?? '300px' },
    );

    io.observe(sentinel);

    // MutationObserver on parent — re-check when DOM changes
    const parent = sentinel.parentElement;
    if (parent) {
      mo = new MutationObserver(() => {
        checkSentinel();
      });
      mo.observe(parent, { childList: true, subtree: true });
    }

    // Resize listener
    const onResize = () => checkSentinel();
    window.addEventListener('resize', onResize);

    // Auto-fill: when feedState.loading flips to false, re-check
    let prevLoading = false;
    const autoFillInterval = setInterval(() => {
      const fs = feedStateRef.current;
      if (fs) {
        if (prevLoading && !fs.loading && !fs.end) {
          requestAnimationFrame(checkSentinel);
        }
        prevLoading = fs.loading;
      }
    }, 200);

    return () => {
      io?.disconnect();
      mo?.disconnect();
      window.removeEventListener('resize', onResize);
      clearInterval(autoFillInterval);
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
}