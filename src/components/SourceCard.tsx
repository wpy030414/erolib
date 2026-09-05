import type { CardStatus } from '@/types';

interface SourceCardProps extends React.HTMLAttributes<HTMLDivElement> {
  id?: string;
  title: string;
  pageCount: number;
  subtitle?: string;
  cover: string | null;
  status?: CardStatus;
}

const ACTIVE = ['pending', 'running', 'paused'];
const RING_R = 16;
const RING_CIRCUM = 2 * Math.PI * RING_R;

export function SourceCard({
  id,
  title,
  pageCount,
  subtitle,
  cover,
  status,
  ...rest
}: SourceCardProps) {
  const isLocal = !!status?.localBookId;
  const isBusy = !!status?.taskId && ACTIVE.includes(status?.taskStatus ?? '');
  const isNew = status !== undefined && !isLocal && !isBusy;

  const hasProgress = (status?.progressTotal ?? 0) > 1;
  const ringOffset = (() => {
    const total = status?.progressTotal ?? 0;
    const ratio = total > 1 ? Math.min(1, (status?.progressCurrent ?? 0) / total) : 0;
    return RING_CIRCUM * (1 - ratio);
  })();

  return (
    <div
      id={id}
      className={`book-card${isBusy ? ' book-card--busy' : ''}`}
      {...rest}
    >
      <div className="book-cover-wrap">
        {cover ? (
          <img src={cover} className="book-cover" alt={title} loading="lazy" decoding="async" />
        ) : (
          <div className="book-placeholder">{(title || '?').charAt(0).toUpperCase()}</div>
        )}
        {pageCount > 0 && <div className="book-pages-badge">{pageCount}</div>}
        {isBusy && (
          <div className="book-cover-mask">
            {hasProgress ? (
              <div className="progress-ring-wrap">
                <svg className="progress-ring" viewBox="0 0 36 36">
                  <circle className="ring-track" cx="18" cy="18" r={RING_R} />
                  <circle
                    className="ring-fill"
                    cx="18"
                    cy="18"
                    r={RING_R}
                    strokeDasharray={RING_CIRCUM}
                    strokeDashoffset={ringOffset}
                  />
                </svg>
                <span className="progress-text">
                  {status?.progressCurrent}/{status?.progressTotal}
                </span>
              </div>
            ) : (
              <svg className="spinner" style={{ color: '#fff' }} viewBox="0 0 50 50" aria-hidden="true">
                <circle className="spinner-track" cx="25" cy="25" r="20" />
                <circle className="spinner-arc" cx="25" cy="25" r="20" />
              </svg>
            )}
          </div>
        )}
      </div>
      <div className="md3-card__content">
        {isNew && <span className="new-dot" aria-hidden="true" />}
        <div className="md3-card__title text-subtitle-2">
          <span className="title-inner">{title}</span>
        </div>
        {subtitle && (
          <div className="md3-card__subtitle text-body-2 text-truncate">
            {subtitle}
          </div>
        )}
      </div>
    </div>
  );
}