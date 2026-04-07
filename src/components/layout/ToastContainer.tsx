import { X, AlertTriangle, CheckCircle, Info } from 'lucide-react';
import { useToastStore } from '../../store/toastStore';

export default function ToastContainer() {
    const { toasts, removeToast } = useToastStore();

    if (toasts.length === 0) return null;

    const iconMap = {
        error: <AlertTriangle className="h-4 w-4 shrink-0 text-red-500" />,
        success: <CheckCircle className="h-4 w-4 shrink-0 text-green-500" />,
        info: <Info className="h-4 w-4 shrink-0 text-blue-500" />,
    };

    const bgMap = {
        error: 'bg-red-50 border-red-200 text-red-800 dark:bg-red-900/20 dark:border-red-800 dark:text-red-200',
        success: 'bg-green-50 border-green-200 text-green-800 dark:bg-green-900/20 dark:border-green-800 dark:text-green-200',
        info: 'bg-blue-50 border-blue-200 text-blue-800 dark:bg-blue-900/20 dark:border-blue-800 dark:text-blue-200',
    };

    return (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[100] flex flex-col gap-2 max-w-sm">
            {toasts.map((toast) => (
                <div
                    key={toast.id}
                    className={`flex items-center gap-2 rounded-lg border px-3 py-2 shadow-md ${bgMap[toast.type]}`}
                >
                    {iconMap[toast.type]}
                    <span className="flex-1 text-sm">{toast.message}</span>
                    <button onClick={() => removeToast(toast.id)} className="shrink-0 opacity-60 hover:opacity-100">
                        <X className="h-3 w-3" />
                    </button>
                </div>
            ))}
        </div>
    );
}
