import { useToastStore } from '@/stores/toast';
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

export function AppToast() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);

  if (toasts.length === 0) return null;

  return (
    <div className="toast-container">
      {toasts.map((msg) => (
        <div
          key={msg.id}
          className="toast"
          role="status"
          onClick={() => dismiss(msg.id)}
        >
          <MdiIcon path={iconFor(msg.kind)} size={18} />
          <span className="toast-message">{msg.message}</span>
        </div>
      ))}
    </div>
  );
}