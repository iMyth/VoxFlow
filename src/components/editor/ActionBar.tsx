import { Volume2, Save } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import { Progress } from '../ui/progress';

interface ActionBarProps {
    isDirty: boolean;
    isBatchTtsRunning: boolean;
    batchTtsProgress: { current: number; total: number } | null;
    missingTtsCount: number;
    onSave: () => void;
    onGenerateAllTts: () => void;
}

export default function ActionBar({
    isDirty,
    isBatchTtsRunning,
    batchTtsProgress,
    missingTtsCount,
    onSave,
    onGenerateAllTts,
}: ActionBarProps) {
    const { t } = useTranslation();

    if (!isDirty && missingTtsCount === 0) return null;

    return (
        <div className="flex gap-2 flex-wrap items-center">
            {isDirty && (
                <Button variant="outline" onClick={onSave}>
                    <Save className="h-4 w-4" /> {t('editor.save')}
                </Button>
            )}
            {missingTtsCount > 0 && (
                <Button
                    variant="outline"
                    onClick={onGenerateAllTts}
                    disabled={isBatchTtsRunning}
                >
                    <Volume2 className="h-4 w-4" />
                    {isBatchTtsRunning
                        ? t('editor.batchTtsRunning', { current: batchTtsProgress?.current ?? 0, total: batchTtsProgress?.total ?? missingTtsCount })
                        : t('editor.generateAllTts', { count: missingTtsCount })}
                </Button>
            )}
            {isBatchTtsRunning && batchTtsProgress && (
                <div className="flex-1 min-w-[120px] space-y-1">
                    <Progress
                        value={(batchTtsProgress.current / batchTtsProgress.total) * 100}
                        className="h-2"
                    />
                    <p className="text-xs text-muted-foreground">
                        {batchTtsProgress.current} / {batchTtsProgress.total}
                    </p>
                </div>
            )}
        </div>
    );
}
