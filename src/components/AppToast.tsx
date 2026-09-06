import { useEffect, useRef, useState } from 'react';
import { useToastStore, type ToastMessage } from '@/stores/toast';
import { MdiIcon } from './MdiIcon';
import {
  mdiCheckCircleOutline,
  mdiAlertCircleOutline,
  mdiInformationOutline,
} from '@mdi/js';

function iconFor(kind: 'success' | 'error' | 'info') {
  if (kind === 'success') return mdiCheckCircleOutline;
  if (kind === 'error') return mdiAlertCircleOutline;
  return mdiInformationOutline;
}

const LEAVE_MS = 150;

/** Vue <TransitionGroup name="toast"> 语义对齐：进入 0.2s（CSS animation），
 *  离场 0.15s 淡出下滑后才移除——离场期间保持占位，兄弟不跳位。 */
export function AppToast() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  // 离场缓存：store 里已消失、但仍在播放离场动画的 toast。
  const [leaving, setLeaving] = useState<ToastMessage[]>([]);
  const prevRef = useRef(toasts);

  // 渲染期对账（React「props 变化时调整状态」模式）：保证 store 清空的同一
  // 帧离场缓存就位，容器不闪断、离场动画从当前画面平滑接管。
  if (prevRef.current !== toasts) {
    const gone = prevRef.current.filter((t) => !toasts.some((n) => n.id === t.id));
    prevRef.current = toasts;
    if (gone.length) {
      const ids = new Set(gone.map((t) => t.id));
      setLeaving((prev) => [...prev.filter((p) => !ids.has(p.id)), ...gone]);
    }
  }

  // 每批离场项到时（0.15s 动画播完）移除。
  useEffect(() => {
    if (!leaving.length) return;
    const ids = leaving.map((t) => t.id);
    const timer = window.setTimeout(() => {
      setLeaving((prev) => prev.filter((p) => !ids.includes(p.id)));
    }, LEAVE_MS);
    return () => window.clearTimeout(timer);
  }, [leaving]);

  // 已在 store 里的（同 id 重新出现）只渲染活动副本，避免重复 key。
  const leavingOnly = leaving.filter((l) => !toasts.some((n) => n.id === l.id));
  if (toasts.length === 0 && leavingOnly.length === 0) return null;

  return (
    <div className="toast-container">
      {toasts.map((msg) => (
        <div
          key={msg.id}
          className="toast"
          role="status"
          onClick={() => dismiss(msg.id)}
        >
          <MdiIcon className="toast-icon" path={iconFor(msg.kind)} size={18} />
          <span className="toast-message">{msg.message}</span>
        </div>
      ))}
      {leavingOnly.map((msg) => (
        <div key={msg.id} className="toast toast--leaving" role="status" aria-hidden>
          <MdiIcon className="toast-icon" path={iconFor(msg.kind)} size={18} />
          <span className="toast-message">{msg.message}</span>
        </div>
      ))}
    </div>
  );
}
