import { create } from 'zustand';

export interface ToastMessage {
  id: number;
  kind: 'success' | 'error' | 'info';
  message: string;
}

interface ToastState {
  toasts: ToastMessage[];
  addToast: (kind: ToastMessage['kind'], message: string) => void;
  dismiss: (id: number) => void;
}

let nextId = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  addToast: (kind, message) => {
    const id = nextId++;
    set((s) => ({ toasts: [...s.toasts, { id, kind, message }] }));
    const timer = setTimeout(() => {
      timers.delete(id);
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    }, 4000);
    timers.set(id, timer);
  },
  dismiss: (id) => {
    const t = timers.get(id);
    if (t) { clearTimeout(t); timers.delete(id); }
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
  },
}));