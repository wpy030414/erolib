import { useInfiniteSentinel } from '@/hooks/useInfiniteSentinel';
import { FeedLoading } from './FeedLoading';

interface FeedListProps {
  feed: { items: unknown[]; loading: boolean; end: boolean };
  texts: { empty: string; end: string; loadingMore: string };
  onLoadMore: () => void;
  children: React.ReactNode;
}

export function FeedList({ feed, texts, onLoadMore, children }: FeedListProps) {
  const sentinelRef = useInfiniteSentinel(() => onLoadMore(), {
    feedState: feed,
  });

  return (
    <div>
      {feed.items.length > 0 && (
        <div className="md3-grid">{children}</div>
      )}
      {feed.items.length === 0 && !feed.loading && (
        <div className="text-center text-medium-emphasis mt-8">{texts.empty}</div>
      )}
      {feed.end && feed.items.length > 0 && (
        <div className="feed-end text-center text-medium-emphasis">{texts.end}</div>
      )}
      {feed.loading && <FeedLoading>{texts.loadingMore}</FeedLoading>}
      <div ref={sentinelRef} className="feed-sentinel" />
    </div>
  );
}