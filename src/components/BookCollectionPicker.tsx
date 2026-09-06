import { useEffect, useState, useRef } from 'react';
import { useI18n } from '@/hooks/useI18n';
import { useCollectionsStore } from '@/stores/collections';
import { M3eDialog } from '@m3e/react/dialog';
import { M3eButton } from '@m3e/react/button';

interface BookCollectionPickerProps {
  bookId: string;
  onClose: () => void;
}

export function BookCollectionPicker({ bookId, onClose }: BookCollectionPickerProps) {
  const { t } = useI18n();
  const store = useCollectionsStore();
  const [checkedIds, setCheckedIds] = useState<Set<string>>(new Set());
  const initialIds = useRef<Set<string>>(new Set());
  // Opens only after the checked state is loaded, so the dialog never shows
  // an all-unchecked list that then snaps to the real state.
  const [open, setOpen] = useState(false);

  useEffect(() => {
    void (async () => {
      store.ensureLoaded();
      const ids = await store.getBookCollections(bookId);
      setCheckedIds(new Set(ids));
      initialIds.current = new Set(ids);
      setOpen(true);
    })();
  }, [bookId, store]);

  function toggle(collectionId: string) {
    setCheckedIds((prev) => {
      const next = new Set(prev);
      if (next.has(collectionId)) {
        next.delete(collectionId);
      } else {
        next.add(collectionId);
      }
      return next;
    });
  }

  /** Dialog 的 closed 事件触发：对齐 Vue @close —— 关闭时同步 store 增量，
   *  确认、Esc、遮罩三条关闭路径在此汇合。m3e 的 closed 在 close() 同一 tick
   *  派发（离场过渡 ≈150ms 尚未播完），因此同步与保底等待并行完成后再通知
   *  父级卸载，保证关闭动画完整播放。 */
  async function handleClosed() {
    const added: string[] = [];
    const removed: string[] = [];
    for (const id of checkedIds) {
      if (!initialIds.current.has(id)) added.push(id);
    }
    for (const id of initialIds.current) {
      if (!checkedIds.has(id)) removed.push(id);
    }
    await Promise.all([
      Promise.all([
        ...added.map((cid) => store.addBookToCollection(cid, bookId)),
        ...removed.map((cid) => store.removeBookFromCollection(cid, bookId)),
      ]),
      new Promise((resolve) => setTimeout(resolve, 200)),
    ]);
    onClose();
  }

  return (
    <M3eDialog open={open} onClosed={handleClosed}>
      <div slot="headline">{t('lib.collections.addToTitle')}</div>
      <div slot="content" className="picker__content">
        {!store.collections.length ? (
          <div className="picker__empty">{t('lib.collections.empty')}</div>
        ) : (
          store.collections.map((col) => (
            <label key={col.id} className="picker__row">
              <input
                type="checkbox"
                className="picker__checkbox"
                checked={checkedIds.has(col.id)}
                onChange={() => toggle(col.id)}
              />
              <span className="picker__name">{col.name}</span>
            </label>
          ))
        )}
      </div>
      <div slot="actions">
        {/* 先关闭（播放内置关闭动画），closed 事件里再做同步 */}
        <M3eButton variant="filled" onClick={() => setOpen(false)}>
          {t('common.confirm')}
        </M3eButton>
      </div>
    </M3eDialog>
  );
}