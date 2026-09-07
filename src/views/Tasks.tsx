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
  const [clearing, setClearing] = useState(false);
  const [retrying, setRetrying] = useState(false);
  const hasCompleted = taskStore.tasks.some((t) => t.status === 'completed');
  const hasRetryable = taskStore.tasks.some((t) => t.status === 'failed' || t.status === 'paused');

  useEffect(() => { void taskStore.init(); }, []);

  function progressPercent(item: TaskItem): number {
    // Unknown total counts as complete; never exceed 100%.
    if (item.progress_total <= 0) return 100;
    return Math.min(100, Math.round((item.progress_current / item.progress_total) * 100));
  }

  /**
   * Task titles are formatted as "{Source}: {actual book title}" (e.g.
   * "ASMHentai: ある作品"). Strip the first "Prefix: " segment so the search
   * matches the bare book title registered in the library.
   */
  function extractBookTitle(taskTitle: string): string {
    return taskTitle.replace(/^[A-Za-z]+:\s*/, '');
  }

  async function viewInLibrary(taskTitle: string) {
    collectionsStore.setActiveCollection(null);
    const title = extractBookTitle(taskTitle);
    navigate(`/library?search=${encodeURIComponent(title)}`);
  }

  async function onClearCompleted() {
    setClearing(true);
    try {
      const before = taskStore.tasks.filter((tk) => tk.status === 'completed').length;
      await taskStore.clearCompleted();
      toast.addToast('info', t('tasks.toast.cleared', { count: before }));
    } catch (e) {
      toast.addToast('error', t('common.error', { message: String(e) }));
    } finally {
      setClearing(false);
    }
  }

  async function onRetryAll() {
    setRetrying(true);
    try {
      await taskStore.retryAll();
      toast.addToast('info', t('tasks.toast.retried'));
    } catch (e) {
      toast.addToast('error', t('common.error', { message: String(e) }));
    } finally {
      setRetrying(false);
    }
  }

  async function onRedownload(item: TaskItem) {
    // Global debounce: only one re-download at a time.
    if (redownloadingId) return;
    setRedownloadingId(item.id);
    try {
      const action = await taskStore.redownloadTask(item.id);
      const title = extractBookTitle(item.title);
      if (action === 'already_complete') toast.addToast('info', t('tasks.toast.alreadyComplete', { title }));
      else toast.addToast('success', t('tasks.toast.redownloadStarted', { title }));
    } catch (e) { toast.addToast('error', t('tasks.toast.redownloadFailed', { message: String(e) })); }
    finally { setRedownloadingId(null); }
  }

  async function copyLogs(item: TaskItem) {
    if (!item.logs.length) return;
    try {
      await writeText(item.logs.join('\n'));
      toast.addToast('success', t('tasks.logs.copied'));
    } catch (e) {
      console.error('[Tasks] copyLogs failed:', e);
      toast.addToast('error', t('tasks.logs.copyFailed'));
    }
  }

  const statusColor: Record<string, string> = {
    running: 'var(--md-sys-color-tertiary-container)', pending: 'var(--md-sys-color-secondary-container)',
    paused: 'var(--md-sys-color-surface-variant)', completed: 'var(--md-sys-color-primary-container)',
    failed: 'var(--md-sys-color-error-container)', cancelled: 'var(--md-sys-color-surface-variant)',
  };

  return (
    <div className="pa-6">
      {/* tasks-header 语义对齐：Vue 中标题在 v-if 链之外，loading/空态也常驻 */}
      <div className="d-flex align-center gap-4 mb-6" style={{ flexWrap: 'wrap', minHeight: 40 }}>
        <h2 className="text-h5" style={{ margin: 0 }}>{t('tasks.title')}</h2>
      </div>
      {taskStore.loading ? (
        <div className="empty-state"><FeedLoading /></div>
      ) : taskStore.tasks.length === 0 ? (
        // empty-state 语义对齐：200px 垂直居中
        <div className="empty-state"><p className="text-body-1 text-medium-emphasis">{t('tasks.empty')}</p></div>
      ) : (
      <div className="task-list">
        {taskStore.tasks.map((item) => (
          <div
            key={item.id}
            className={`md3-card md3-card--outlined task-card${taskStore.selectedTaskId === item.id ? ' task-card--selected' : ''}`}
            onClick={() => taskStore.selectTask(item.id)}
          >
            <div className="d-flex align-center" style={{ gap: 12 }}>
              <span className="text-truncate" style={{ flex: 1, fontWeight: 500, minWidth: 0 }}>{item.title}</span>
              <span style={{ padding: '2px 10px', borderRadius: 'var(--md-sys-shape-corner-small)', fontSize: 12, background: statusColor[item.status] || 'var(--md-sys-color-surface-container)', whiteSpace: 'nowrap' }}>
                {t('tasks.status.' + item.status)}
              </span>
            </div>
            <div className="d-flex align-center" style={{ gap: 12 }}>
              <div style={{ flex: 1, height: 6, borderRadius: 3, background: 'var(--md-sys-color-surface-variant)', overflow: 'hidden' }}>
                <div style={{ width: progressPercent(item) + '%', height: '100%', borderRadius: 3, background: 'var(--md-sys-color-primary)', transition: 'width 0.3s ease' }} />
              </div>
              <span style={{ minWidth: 36, textAlign: 'right', color: 'var(--md-sys-color-on-surface-variant)' }}>{progressPercent(item)}%</span>
            </div>
            {/* task-logs-wrap 语义对齐：选中展开时 0.2s 淡入 */}
            {taskStore.selectedTaskId === item.id && (
              <div className="task-logs-body" onContextMenu={(e) => { e.preventDefault(); copyLogs(item); }} style={{ maxHeight: 220, overflowY: 'auto', fontSize: 12, padding: '8px 10px', borderRadius: 'var(--md-sys-shape-corner-small)', background: 'var(--md-sys-color-surface-variant)', color: 'var(--md-sys-color-on-surface-variant)', fontFamily: 'var(--md-sys-typescale-font)', fontVariantNumeric: 'tabular-nums', wordBreak: 'break-word', whiteSpace: 'pre-wrap' }}>
                {item.logs.length > 0 ? item.logs.map((line, i) => <div key={i} style={{ marginBottom: 2 }}>{line}</div>) : <div>{t('tasks.detail.noLogs')}</div>}
              </div>
            )}
            {/* task-footer 语义对齐：按钮行 + 右侧速度/摘要同行（space-between） */}
            <div className="task-footer">
              <div className="task-actions">
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
                  <M3eButton variant="outlined" disabled={redownloadingId === item.id} onClick={(e) => { e.stopPropagation(); void onRedownload(item); }} style={{ fontSize: 13 }}>
                    <MdiIcon path={mdiDownload} size={18} /> {t('tasks.actions.redownload')}
                  </M3eButton>
                )}
                {(item.status === 'completed' || item.status === 'failed' || item.status === 'cancelled') && (
                  <M3eButton variant="outlined" onClick={(e) => { e.stopPropagation(); void taskStore.deleteTask(item.id); }} style={{ fontSize: 13 }}>
                    <MdiIcon path={mdiDelete} size={18} /> {t('tasks.actions.remove')}
                  </M3eButton>
                )}
              </div>
              {item.status === 'running' && (
                <span className="task-speed">{formatSpeed(item.speed, t)}</span>
              )}
              {item.status === 'completed' && (
                <span className="task-speed">
                  {t('tasks.summary', { size: formatBytes(item.total_bytes, t), time: formatDuration(item.elapsed_ms, t) })}
                </span>
              )}
            </div>
          </div>
        ))}
      </div>
      )}
      <>
      {hasRetryable && <FabButton icon={mdiRestart} ariaLabel={t('tasks.actions.retryAll')} disabled={retrying} style={{ bottom: 96 }} onClick={() => { void onRetryAll(); }} />}
      {hasCompleted && <FabButton icon={mdiBroom} ariaLabel={t('tasks.actions.clearCompleted')} disabled={clearing} onClick={() => { void onClearCompleted(); }} />}
      </>
    </div>
  );
}