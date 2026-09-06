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
 *  离场 0.15s 淡出下滑后才移除——离场期间 toast 保持占位，兄弟不跳位。 */
export function AppToast() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);
  // 离场缓存：store 里已消失、但仍在播放离场动画的 toast。
  const [leaving, setLeaving] = useState<ToastMessage[]>([]);
  const prevRef = useRef(toasts);

  useEffect(() => {
    const gone = prevRef.current.filter((t) => !toasts.some((n) => n.id === t.id));
    prevRef.current = toasts;
    if (gone.length === 0) return;
    setLeaving((prev) => [...prev, ...gone]);
    const ids = gone.map((t) => t.id);
    const timer = window.setTimeout(() => {
      setLeaving((prev) => prev.filter((t) => !ids.includes(t.id)));
    }, LEAVE_MS);
    return () => window.clearTimeout(timer);
  }, [toasts]);

  if (toasts.length === 0 && leaving.length === 0) return null;

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
      {leaving.map((msg) => (
        <div key={msg.id} className="toast toast--leaving" role="status" aria-hidden>
          <MdiIcon className="toast-icon" path={iconFor(msg.kind)} size={18} />
          <span className="toast-message">{msg.message}</span>
        </div>
      ))}
    </div>
  );
}
