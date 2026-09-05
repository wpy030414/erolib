import { forwardRef, useImperativeHandle, useRef, useState, useCallback } from 'react';
import { save as dialogSave } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  mdiClose,
  mdiArchive,
  mdiBookOpen,
  mdiFilePdfBox,
} from '@mdi/js';
import { useI18n } from '@/hooks/useI18n';
import { useToastStore } from '@/stores/toast';
import { api, type ExportProgress } from '@/services/api';
import { MdiIcon } from './MdiIcon';
import { M3eButton } from '@m3e/react/button';
import { M3eLinearProgressIndicator } from '@m3e/react/progress-indicator';
import type { Book } from '@/types';

export interface BookExportDialogHandle {
  open: (book: Book) => void;
  close: () => void;
}

export const BookExportDialog = forwardRef<BookExportDialogHandle>((_, ref) => {
  const { t } = useI18n();
  const toast = useToastStore();
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [book, setBook] = useState<Book | null>(null);
  const [selected, setSelected] = useState<'cb7' | 'epub' | 'pdf'>('cb7');
  const [busy, setBusy] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState({ done: 0, total: 0 });
  const activeBookId = useRef('');
  const unlistenRef = useRef<UnlistenFn | null>(null);

  const progressValue = (progress.done + 1) / (progress.total + 1);

  const formats = [
    { value: 'cb7' as const, icon: mdiArchive, label: t('lib.save.format.cb7'), desc: '.cb7' },
    { value: 'epub' as const, icon: mdiBookOpen, label: t('lib.save.format.epub'), desc: '.epub' },
    { value: 'pdf' as const, icon: mdiFilePdfBox, label: t('lib.save.format.pdf'), desc: '.pdf' },
  ];

  const open = useCallback((b: Book) => {
    setBook(b);
    setSelected(b.format === 'cb7' || b.format === 'epub' || b.format === 'pdf' ? b.format : 'cb7');
    dialogRef.current?.showModal();
  }, []);

  const close = useCallback(() => {
    if (busy) return;
    dialogRef.current?.close();
    setBook(null);
  }, [busy]);

  useImperativeHandle(ref, () => ({ open, close }), [open, close]);

  function stopListening() {
    unlistenRef.current?.();
    unlistenRef.current = null;
    activeBookId.current = '';
  }

  function onEscape(e: React.KeyboardEvent) {
    if (busy && e.key === 'Escape') e.preventDefault();
  }

  function onBackdrop(e: React.MouseEvent) {
    if (e.target === e.currentTarget) close();
  }

  async function confirm() {
    if (!book || busy) return;
    setBusy(true);
    const b = book;
    const fmt = selected;
    const defaultName = `${b.title || 'book'}.${fmt}`;
    try {
      const dest = await dialogSave({
        defaultPath: defaultName,
        filters: [
          { name: fmt.toUpperCase(), extensions: [fmt] },
          { name: t('lib.save.allFiles'), extensions: ['*'] },
        ],
      });
      if (!dest) { setBusy(false); return; }

      setExporting(true);
      setProgress({ done: 0, total: Math.max(b.page_count, 0) });
      activeBookId.current = b.id;
      stopListening();
      unlistenRef.current = await listen<ExportProgress>('book://export-progress', (event) => {
        const p = event.payload;
        if (p.book_id !== activeBookId.current) return;
        setProgress({ done: p.done, total: p.total });
      });

      await api.saveBook(b.id, dest, fmt);
      toast.addToast('success', t('lib.saved', { title: b.title }));
      dialogRef.current?.close();
      setBook(null);
    } catch (e) {
      toast.addToast('error', t('lib.saveFailed', { error: String(e) }));
    } finally {
      stopListening();
      setExporting(false);
      setBusy(false);
    }
  }

  if (!book) return null;

  return (
    <dialog ref={dialogRef} className="export-dialog" onClick={onBackdrop} onKeyDown={onEscape}>
      <div className="export-dialog__panel">
        <div className="export-dialog__header">
          <span className="export-dialog__title">
            {exporting ? t('lib.save.exporting') : t('lib.save.format')}
          </span>
          {!exporting && (
            <button className="icon-btn" aria-label={t('common.dismiss')} onClick={close}>
              <MdiIcon path={mdiClose} size={20} />
            </button>
          )}
        </div>

        <p className="export-dialog__subtitle">{book.title}</p>

        {!exporting ? (
          <>
            <div className="format-options">
              {formats.map((opt) => (
                <button
                  key={opt.value}
                  className={`format-option${selected === opt.value ? ' format-option--selected' : ''}`}
                  onClick={() => setSelected(opt.value)}
                >
                  <MdiIcon path={opt.icon} size={22} />
                  <span className="format-option__label">{opt.label}</span>
                  <span className="format-option__desc">{opt.desc}</span>
                </button>
              ))}
            </div>
            <div className="export-dialog__actions">
              <M3eButton variant="outlined" onClick={close}>
                {t('common.cancel')}
              </M3eButton>
              <M3eButton variant="filled" disabled={busy} onClick={confirm}>
                {t('lib.save')}
              </M3eButton>
            </div>
          </>
        ) : (
          <div className="export-progress">
            <M3eLinearProgressIndicator value={progressValue} />
            <p className="export-progress__label">
              {t('lib.save.exportingProgress', { done: progress.done, total: progress.total })}
            </p>
          </div>
        )}
      </div>
    </dialog>
  );
});