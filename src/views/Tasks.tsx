import { useEffect, useState, useRef } from 'react';
import { useI18n } from '@/hooks/useI18n';
import { useTaskStore } from '@/stores/tasks';
import { useToastStore } from '@/stores/toast';
import { useCollectionsStore } from '@/stores/collections';
import { useNavigate } from 'react-router-dom';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { MdiIcon } from '@/components/MdiIcon';
import { FabButton } from '@/components/FabButton';
import { FeedLoading } from '@/components/FeedLoading';
import { formatBytes, formatSpeed, formatDuration } from '@/utils/format';
import {
  mdiPause, mdiPlay, mdiClose, mdiDelete, mdiRefresh, mdiMagnify, mdiBroom, mdiRestart, mdiDownload,
} from '@mdi/js';
import { M3eButton } from '@m3e/react/button';
import type { TaskItem } from '@/services/api';

export default function Tasks() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const taskStore = useTaskStore();
  const toast = useToastStore();
  const collectionsStore = useCollectionsStore();
  const [redownloadingId, setRedownloadingId] = useState<string | null>(null);
  const hasCompleted = taskStore.tasks.some((t) => t.status === 'completed');
  const hasRetryable = taskStore.tasks.some((t) => t.status === 'failed' || t.status === 'paused');

  useEffect(() => { void taskStore.init(); }, []);

  function progressPercent(item: TaskItem): number {
    if (item.progress_total <= 0) return 0;
    return Math.round((item.progress_current / item.progress_total) * 100);
  }

  function extractBookTitle(taskTitle: string): string {
    return taskTitle.replace(/^(Pixiv|EHentai|AHentai|NiceCat):\s*/, '');
  }

  async function viewInLibrary(taskTitle: string) {
    collectionsStore.setActiveCollection(null);
    const title = extractBookTitle(taskTitle);
    navigate(`/library?search=${encodeURIComponent(title)}`);
  }

  async function onClearCompleted() {
    const count = await taskStore.clearCompleted();
  }

  async function onRedownload(item: TaskItem) {
    if (redownloadingId) return;
    setRedownloadingId(item.id);
    try {
      const action = await taskStore.redownloadTask(item.id);
      if (action === 'already_complete') toast.addToast('info', t('tasks.toast.alreadyComplete'));
      else toast.addToast('success', t('tasks.toast.redownloaded'));
    } catch (e) { toast.addToast('error', String(e)); }
    finally { setRedownloadingId(null); }
  }

  async function copyLogs(item: TaskItem) {
    try { await writeText(item.logs.join('\n')); }
    catch { /* ignore */ }
  }

  const statusColor: Record<string, string> = {
    running: 'var(--md-sys-color-tertiary-container)', pending: 'var(--md-sys-color-secondary-container)',
    paused: 'var(--md-sys-color-surface-container-highest)', completed: 'var(--md-sys-color-primary-container)',
    failed: 'var(--md-sys-color-error-container)', cancelled: 'var(--md-sys-color-surface-container-highest)',
  };

  if (taskStore.loading) return <div className="pa-6"><FeedLoading /></div>;
  if (taskStore.tasks.length === 0) return <div className="pa-6 text-center text-medium-emphasis mt-8">{t('tasks.empty')}</div>;

  return (
    <div className="pa-6">
      <div className="d-flex align-center gap-4 mb-6">
        <h2 className="text-h5" style={{ margin: 0 }}>{t('tasks.title')}</h2>
      </div>
      <div className="task-list">
        {taskStore.tasks.map((item) => (
          <div
            key={item.id}
            className={`md3-card md3-card--outlined task-card${taskStore.selectedTaskId === item.id ? ' task-card--selected' : ''}`}
            onClick={() => taskStore.selectTask(item.id)}
            style={{ padding: 16, marginBottom: 12, borderRadius: 'var(--md-sys-shape-corner-medium)', border: '1px solid var(--md-sys-color-outline-variant)', cursor: 'pointer' }}
          >
            <div className="d-flex align-center" style={{ gap: 8, marginBottom: 8 }}>
              <span className="text-truncate" style={{ flex: 1, fontWeight: 500 }}>{item.title}</span>
              <span style={{ padding: '2px 10px', borderRadius: 'var(--md-sys-shape-corner-full)', fontSize: 12, background: statusColor[item.status] || 'var(--md-sys-color-surface-container)', whiteSpace: 'nowrap' }}>
                {t('tasks.status.' + item.status)}
              </span>
            </div>
            <div className="d-flex align-center" style={{ gap: 8, marginBottom: 8 }}>
              <div style={{ flex: 1, height: 6, borderRadius: 3, background: 'var(--md-sys-color-surface-container-highest)', overflow: 'hidden' }}>
                <div style={{ width: progressPercent(item) + '%', height: '100%', borderRadius: 3, background: 'var(--md-sys-color-primary)', transition: 'width 0.3s ease' }} />
              </div>
              <span style={{ fontSize: 12, minWidth: 36, textAlign: 'right' }}>{progressPercent(item)}%</span>
            </div>
            {taskStore.selectedTaskId === item.id && (
              <div onContextMenu={(e) => { e.preventDefault(); copyLogs(item); }} style={{ maxHeight: 200, overflowY: 'auto', fontSize: 12, padding: 8, borderRadius: 8, background: 'var(--md-sys-color-surface-container)', color: 'var(--md-sys-color-on-surface-variant)', marginBottom: 8, whiteSpace: 'pre-wrap' }}>
                {item.logs.length > 0 ? item.logs.map((line, i) => <div key={i}>{line}</div>) : <div>{t('tasks.detail.noLogs')}</div>}
              </div>
            )}
            <div className="d-flex align-center" style={{ gap: 8, flexWrap: 'wrap' }}>
              {item.status === 'completed' && item.book_id && (
                <M3eButton variant="filled" onClick={(e) => { e.stopPropagation(); viewInLibrary(item.title); }} style={{ fontSize: 13 }}>
                  <MdiIcon path={mdiMagnify} size={18} /> {t('tasks.actions.view')}
                </M3eButton>
              )}
              {item.status === 'running' && (
                <M3eButton variant="tonal" onClick={(e) => { e.stopPropagation(); void taskStore.pauseTask(item.id); }} style={{ fontSize: 13 }}>
                  <MdiIcon path={mdiPause} size={18} /> {t('tasks.actions.pause')}
                </M3eButton>
              )}
              {item.status === 'paused' && (
                <M3eButton variant="tonal" onClick={(e) => { e.stopPropagation(); void taskStore.resumeTask(item.id); }} style={{ fontSize: 13 }}>
                  <MdiIcon path={mdiPlay} size={18} /> {t('tasks.actions.resume')}
                </M3eButton>
              )}
              {(item.status === 'running' || item.status === 'paused') && (
                <M3eButton variant="tonal" onClick={(e) => { e.stopPropagation(); void taskStore.cancelTask(item.id); }} style={{ fontSize: 13 }}>
                  <MdiIcon path={mdiClose} size={18} /> {t('tasks.actions.cancel')}
                </M3eButton>
              )}
              {item.status === 'failed' && (
                <M3eButton variant="tonal" onClick={(e) => { e.stopPropagation(); void taskStore.retryTask(item.id); }} style={{ fontSize: 13 }}>
                  <MdiIcon path={mdiRefresh} size={18} /> {t('tasks.actions.retry')}
                </M3eButton>
              )}
              {item.status === 'completed' && (
                <M3eButton variant="tonal" onClick={(e) => { e.stopPropagation(); void onRedownload(item); }} style={{ fontSize: 13 }}>
                  <MdiIcon path={mdiDownload} size={18} /> {t('tasks.actions.redownload')}
                </M3eButton>
              )}
              {(item.status === 'completed' || item.status === 'failed' || item.status === 'cancelled') && (
                <M3eButton variant="tonal" onClick={(e) => { e.stopPropagation(); void taskStore.deleteTask(item.id); }} style={{ fontSize: 13 }}>
                  <MdiIcon path={mdiDelete} size={18} /> {t('tasks.actions.remove')}
                </M3eButton>
              )}
              {(item.status === 'running' || item.status === 'paused') && item.speed > 0 && (
                <span style={{ fontSize: 12, marginLeft: 'auto', color: 'var(--md-sys-color-on-surface-variant)' }}>
                  {formatSpeed(item.speed, t)} · {formatBytes(item.total_bytes, t)}
                </span>
              )}
            </div>
          </div>
        ))}
      </div>
      <>
      {hasCompleted && <FabButton icon={mdiBroom} ariaLabel={t('tasks.actions.clearCompleted')} onClick={() => { void onClearCompleted(); }} />}
      {hasRetryable && <FabButton icon={mdiRestart} ariaLabel={t('tasks.actions.retryAll')} onClick={() => { void taskStore.retryAll(); }} style={hasCompleted ? { bottom: 96 } : undefined} />}
      </>
    </div>
  );
}