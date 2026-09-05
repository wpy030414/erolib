interface FeedLoadingProps {
  children?: React.ReactNode;
}

export function FeedLoading({ children }: FeedLoadingProps) {
  return (
    <div className="feed-loading text-center text-medium-emphasis">
      <svg className="spinner" style={{ color: 'var(--md-sys-color-primary)' }} viewBox="0 0 50 50" aria-hidden="true">
        <circle className="spinner-track" cx="25" cy="25" r="20" />
        <circle className="spinner-arc" cx="25" cy="25" r="20" />
      </svg>
      <span>{children}</span>
    </div>
  );
}