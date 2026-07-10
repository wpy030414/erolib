import { ref } from 'vue';

export interface ToastMessage {
  id: number;
  kind: 'success' | 'error' | 'info';
  message: string;
}

let nextId = 1;

const toasts = ref<ToastMessage[]>([]);

export function useToastStore() {
  function addToast(kind: ToastMessage['kind'], message: string) {
    const id = nextId++;
    toasts.value.push({ id, kind, message });
    setTimeout(() => {
      const idx = toasts.value.findIndex((t) => t.id === id);
      if (idx !== -1) toasts.value.splice(idx, 1);
    }, 4000);
  }

  function dismiss(id: number) {
    const idx = toasts.value.findIndex((t) => t.id === id);
    if (idx !== -1) toasts.value.splice(idx, 1);
  }

  return { toasts, addToast, dismiss };
}
