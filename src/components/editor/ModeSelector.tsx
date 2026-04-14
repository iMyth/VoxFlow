import { Bot, Pencil } from 'lucide-react';
import { useTranslation } from 'react-i18next';

interface ModeSelectorProps {
  onSelectAi: () => void;
  onSelectManual: () => void;
  currentMode: 'ai' | 'manual' | null;
}

export default function ModeSelector({ onSelectAi, onSelectManual, currentMode }: ModeSelectorProps) {
  const { t } = useTranslation();

  return (
    <div className="inline-flex rounded-lg border p-0.5 bg-muted/50">
      <button
        className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md transition-colors ${
          currentMode === 'ai'
            ? 'bg-background shadow-sm text-blue-600 dark:text-blue-400'
            : 'text-muted-foreground hover:text-foreground'
        }`}
        onClick={onSelectAi}
      >
        <Bot className="h-3.5 w-3.5" />
        {t('editor.aiModeShort')}
      </button>
      <button
        className={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-md transition-colors ${
          currentMode === 'manual'
            ? 'bg-background shadow-sm text-amber-600 dark:text-amber-400'
            : 'text-muted-foreground hover:text-foreground'
        }`}
        onClick={onSelectManual}
      >
        <Pencil className="h-3.5 w-3.5" />
        {t('editor.manualModeShort')}
      </button>
    </div>
  );
}
