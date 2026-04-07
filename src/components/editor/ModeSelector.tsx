import { Bot, Pencil } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface ModeSelectorProps {
    onSelectAi: () => void;
    onSelectManual: () => void;
}

export default function ModeSelector({ onSelectAi, onSelectManual }: ModeSelectorProps) {
    const { t } = useTranslation();

    return (
        <div className="grid grid-cols-2 gap-4">
            <button
                className="flex flex-col items-center gap-3 p-6 rounded-lg border-2 border-blue-200 dark:border-blue-800 bg-blue-50 dark:bg-blue-900/20 hover:bg-blue-100 dark:hover:bg-blue-900/40 transition-colors text-left"
                onClick={onSelectAi}
            >
                <Bot className="h-8 w-8 text-blue-600 dark:text-blue-400" />
                <div>
                    <p className="text-sm font-semibold">{t('editor.aiModeTitle')}</p>
                    <p className="text-xs text-muted-foreground mt-1">{t('editor.aiModeFullDesc')}</p>
                </div>
            </button>
            <button
                className="flex flex-col items-center gap-3 p-6 rounded-lg border-2 border-amber-200 dark:border-amber-800 bg-amber-50 dark:bg-amber-900/20 hover:bg-amber-100 dark:hover:bg-amber-900/40 transition-colors text-left"
                onClick={onSelectManual}
            >
                <Pencil className="h-8 w-8 text-amber-600 dark:text-amber-400" />
                <div>
                    <p className="text-sm font-semibold">{t('editor.manualModeTitle')}</p>
                    <p className="text-xs text-muted-foreground mt-1">{t('editor.manualModeFullDesc')}</p>
                </div>
            </button>
        </div>
    );
}
