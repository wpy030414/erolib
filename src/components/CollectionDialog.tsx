import { useEffect, useState, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import {
  mdiPlus,
  mdiDelete,
  mdiPin,
} from '@mdi/js';
import { useI18n } from '@/hooks/useI18n';
import { useCollectionsStore } from '@/stores/collections';
import { useToastStore } from '@/stores/toast';
import { api } from '@/services/api';
import { MdiIcon } from './MdiIcon';
import { M3eDialog } from '@m3e/react/dialog';
import { M3eButton } from '@m3e/react/button';

interface CollectionDialogProps {
  visible: boolean;
  onClose: () => void;
}

export function CollectionDialog({ visible, onClose }: CollectionDialogProps) {
  const { t } = useI18n();
  const store = useCollectionsStore();
  const toast = useToastStore();

  const [collectionCounts, setCollectionCounts] = useState<Record<string, number>>({});
  const [totalBookCount, setTotalBookCount] = useState(0);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const renameInputRef = useRef<HTMLInputElement>(null);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null);
  const [deleteTargetName, setDeleteTargetName] = useState('');

  useEffect(() => {
    store.ensureLoaded();
  }, [store]);

  useEffect(() => {
    if (!visible) return;
    store.ensureLoaded();
    refreshCounts();
  }, [visible, store]);

  async function refreshCounts() {
    api.searchBooks({ sort_by: 'date', sort_order: 'desc', page: 1, page_size: 1 })
      .then((r) => setTotalBookCount(r.total))
      .catch(() => {});
    for (const col of store.collections) {
      api.searchBooks({
        sort_by: 'date', sort_order: 'desc', page: 1, page_size: 1,
        collections: [col.name],
      }).then((r) => setCollectionCounts((prev) => ({ ...prev, [col.id]: r.total })))
        .catch(() => {});
    }
  }

  function selectAll() {
    store.setActiveCollection(null);
    onClose();
  }

  function selectCollection(id: string) {
    if (renamingId) return;
    store.setActiveCollection(id);
    onClose();
  }

  function startRename(col: { id: string; name: string }) {
    setRenamingId(col.id);
    setRenameValue(col.name);
    setTimeout(() => {
      setTimeout(() => {
        renameInputRef.current?.focus();
        renameInputRef.current?.select();
      }, 0);
    }, 0);
  }

  async function commitRename(col: { id: string; name: string }) {
    const name = renameValue.trim();
    setRenamingId(null);
    if (name && name !== col.name) {
      await store.renameCollection(col.id, name);
    }
  }

  function cancelRename() {
    setRenamingId(null);
  }

  function onDeleteClick() {
    if (!renamingId) return;
    const col = store.collections.find((c) => c.id === renamingId);
    if (!col) return;
    setDeleteTargetId(col.id);
    setDeleteTargetName(col.name);
    setDeleteOpen(true);
  }

  async function confirmDelete() {
    const id = deleteTargetId;
    const name = deleteTargetName;
    setDeleteTargetId(null);
    setDeleteTargetName('');
    setRenamingId(null);
    setDeleteOpen(false);
    if (id) {
      const ok = await store.deleteCollection(id);
      if (ok) {
        toast.addToast('success', t('lib.collections.deleted', { name }));
      }
    }
  }

  function cancelDelete() {
    setDeleteTargetId(null);
    setDeleteTargetName('');
    setDeleteOpen(false);
  }

  const MAX_COLLECTIONS = 100;

  async function onCreate() {
    if (store.collections.length >= MAX_COLLECTIONS) {
      toast.addToast('error', t('lib.collections.maxReached', { max: MAX_COLLECTIONS }));
      return;
    }
    const baseName = t('lib.collections.defaultName');
    const existingNames = new Set(store.collections.map((c) => c.name));
    let name = baseName;
    for (let i = 1; existingNames.has(name); i++) {
      name = `${baseName} ${i}`;
    }
    const col = await store.createCollection(name);
    if (col) {
      toast.addToast('success', t('lib.collections.created', { name }));
    }
    setTimeout(refreshCounts, 100);
  }

  if (!visible) return null;

  return createPortal(
    <>
      <div
        className={`drawer-overlay${visible ? ' drawer-overlay--visible' : ''}`}
        onClick={onClose}
      />
      <aside className={`collection-drawer${visible ? ' collection-drawer--open' : ''}`}>
        <h2 className="drawer-title">{t('lib.collections.title')}</h2>

        <div className={`drawer-list${renamingId ? ' drawer-list--masked' : ''}`}>
          {renamingId && <div className="drawer-list__mask" />}

          {/* "All" item */}
          <div
            className={`drawer-item drawer-item--all${store.isAllActive ? ' drawer-item--active' : ''}${renamingId ? ' drawer-item--dimmed' : ''}`}
            onClick={selectAll}
          >
            <span className="drawer-item__name">{t('lib.collections.all')}</span>
            <MdiIcon path={mdiPin} size={16} aria-hidden="true" />
            <span className="drawer-item__count">{totalBookCount}</span>
          </div>

          {/* User collections */}
          {store.collections.map((col) => (
            <div
              key={col.id}
              className={`drawer-item${store.activeCollectionId === col.id ? ' drawer-item--active' : ''}${renamingId === col.id ? ' drawer-item--renaming' : ''}${renamingId && renamingId !== col.id ? ' drawer-item--dimmed' : ''}`}
              onClick={() => selectCollection(col.id)}
              onContextMenu={(e) => { e.preventDefault(); startRename(col); }}
            >
              {renamingId === col.id ? (
                <input
                  ref={renameInputRef}
                  className="drawer-item__input"
                  value={renameValue}
                  maxLength={60}
                  onChange={(e) => setRenameValue(e.target.value)}
                  onBlur={() => commitRename(col)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') commitRename(col);
                    if (e.key === 'Escape') cancelRename();
                  }}
                  onClick={(e) => e.stopPropagation()}
                />
              ) : (
                <span className="drawer-item__name">{col.name}</span>
              )}
              <span className="drawer-item__count">{collectionCounts[col.id] ?? 0}</span>
            </div>
          ))}

          {!store.collections.length && (
            <div className="drawer-empty">{t('lib.collections.empty')}</div>
          )}
        </div>

        {/* Bottom FAB */}
        <div className={`drawer-fab-wrap${renamingId ? ' drawer-fab-wrap--elevated' : ''}`}>
          {renamingId ? (
            <button className="drawer-fab drawer-fab--delete" onClick={onDeleteClick}>
              <MdiIcon path={mdiDelete} size={24} />
            </button>
          ) : (
            <button className="drawer-fab" onClick={onCreate}>
              <MdiIcon path={mdiPlus} size={24} />
            </button>
          )}
        </div>
      </aside>

      {/* Delete confirmation dialog */}
      <M3eDialog open={deleteOpen} onClosed={cancelDelete}>
        <div slot="headline">{t('lib.collections.delete')}</div>
        <div slot="content" className="delete-dialog__content">
          {t('lib.collections.confirmDelete', { name: deleteTargetName })}
        </div>
        <div slot="actions">
          <M3eButton variant="text" onClick={cancelDelete}>{t('common.cancel')}</M3eButton>
          <M3eButton variant="filled" onClick={confirmDelete}>{t('common.confirm')}</M3eButton>
        </div>
      </M3eDialog>
    </>,
    document.body,
  );
}