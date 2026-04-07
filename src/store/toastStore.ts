import { create } from 'zustand';

interface Toast {
    id: number;
    message: string;
    type: 'error' | 'success' | 'info';
}

interface ToastStore {
    toasts: Toast[];
    addToast: (message: string, type?: 'error' | 'success' | 'info') => void;
    removeToast: (id: number) => void;
}

let nextId = 0;

export const useToastStore = create<ToastStore>((set) => ({
    toasts: [],
    addToast: (message, type = 'error') => {
        const id = nextId++;
        set((state) => ({ toasts: [...state.toasts, { id, message, type }] }));
        // Auto-remove after 5 seconds
        setTimeout(() => {
            set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }));
        }, 5000);
    },
    removeToast: (id) => {
        set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }));
    },
}));
