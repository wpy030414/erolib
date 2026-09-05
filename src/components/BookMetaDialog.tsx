import { forwardRef, useImperativeHandle, useRef, useState, useCallback } from 'react';
import { mdiClose } from '@mdi/js';
import { useI18n } from '@/hooks/useI18n';
import { api } from '@/services/api';
import { formatSize } from '@/utils/format';
import { MdiIcon } from './MdiIcon';
import type { Book } from '@/types';

export interface BookMetaDialogHandle {
  open: (book: Book) => void;
  close: () => void;
}

export const BookMetaDialog = forwardRef<BookMetaDialogHandle>((_, ref) => {
  const { t } = useI18n();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [book, setBook] = useState<Book | null>(null);

  const open = useCallback((b: Book) => {
    setBook(b);
    dialogRef.current?.showModal();
    // Re-fetch for locale-current tags
    void api.getBook(b.id).then((fresh) => {
      setBook((prev) => prev?.id === fresh.id ? fresh : prev);
    }).catch(() => {});
  }, []);

  const close = useCallback(() => {
    dialogRef.current?.close();
    setBook(null);
  }, []);

  useImperativeHandle(ref, () => ({ open, close }), [open, close]);

  function onBackdrop(e: React.MouseEvent) {
    if (e.target === e.currentTarget) close();
  }

  const tagList = (book?.tags ?? '').split(',').map((s) => s.trim()).filter(Boolean);

  function formatDate(iso?: string): string {
    if (!iso) return '';
    const d = new Date(iso);
    if (!Number.isNaN(d.getTime())) return d.toLocaleDateString();
    const m = iso.match(/^\d{4}-\d{2}-\d{2}/);
    return m ? m[0] : iso;
  }

  if (!book) return null;

  return (
    <dialog ref={dialogRef} className="meta-dialog" onClick={onBackdrop}>
      <div className="meta-dialog__panel">
        <div className="meta-dialog__header">
          <span className="meta-dialog__title">{t('lib.viewMeta')}</span>
          <button className="icon-btn" aria-label={t('common.dismiss')} onClick={close}>
            <MdiIcon path={mdiClose} size={20} />
          </button>
        </div>
        <dl className="meta-list">
          <dt>{t('lib.meta.title')}</dt>
          <dd>{book.title || '—'}</dd>
          <dt>{t('lib.meta.author')}</dt>
          <dd>{book.author || '—'}</dd>
          <dt>{t('lib.meta.tags')}</dt>
          <dd>
            {tagList.length > 0 ? (
              <div className="tag-chips">
                {tagList.map((tag) => (
                  <span key={tag} className="tag-chip tag-chip--readonly">{tag}</span>
                ))}
              </div>
            ) : <span>—</span>}
          </dd>
          <dt>{t('lib.meta.published')}</dt>
          <dd>{formatDate(book.published_at) || '—'}</dd>
          <dt>{t('lib.meta.source')}</dt>
          <dd>{book.source_plugin || '—'}</dd>
          <dt>{t('lib.meta.postId')}</dt>
          <dd>{book.source_post_id || '—'}</dd>
          <dt>{t('lib.meta.sourceUrl')}</dt>
          <dd>
            {book.source_url ? (
              <a className="meta-link" href={book.source_url} target="_blank" rel="noreferrer">
                {book.source_url}
              </a>
            ) : <span>—</span>}
          </dd>
          <dt>{t('lib.meta.format')}</dt>
          <dd>{(book.format || '').toUpperCase() || '—'}</dd>
          <dt>{t('lib.meta.pages')}</dt>
          <dd>{String(book.page_count ?? 0)}</dd>
          <dt>{t('lib.meta.size')}</dt>
          <dd>{formatSize(book.file_size)}</dd>
          <dt>{t('lib.meta.imported')}</dt>
          <dd>{formatDate(book.created_at)}</dd>
          <dt>{t('lib.meta.scraped')}</dt>
          <dd>{formatDate(book.scraped_at) || '—'}</dd>
        </dl>
      </div>
    </dialog>
  );
});