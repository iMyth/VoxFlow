import { BrainCircuit, Sparkles, Square } from 'lucide-react';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
} from '../ui/dialog';

interface OutlinePanelProps {
    outline: string;
    onOutlineChange: (value: string) => void;
    isAnalyzing: boolean;
    enableThinking: boolean;
    onToggleThinking: (v: boolean) => void;
    onAnalyze: () => void;
    onCancel: () => void;
    hasAgentPlan: boolean;
    open: boolean;
    onOpenChange: (open: boolean) => void;
}

export default function OutlinePanel({
    outline,
    onOutlineChange,
    isAnalyzing,
    enableThinking,
    onToggleThinking,
    onAnalyze,
    onCancel,
    hasAgentPlan,
    open,
    onOpenChange,
}: OutlinePanelProps) {
    const { t } = useTranslation();

    // Auto-close dialog when analysis starts streaming
    useEffect(() => {
        if (isAnalyzing && open) {
            onOpenChange(false);
        }
    }, [isAnalyzing, open, onOpenChange]);

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-w-3xl max-h-[85vh]">
                <DialogHeader>
                    <DialogTitle>{t('editor.outlineEdit')}</DialogTitle>
                </DialogHeader>

                <div className="space-y-3">
                    <textarea
                        className="w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 outline-none resize-y min-h-[200px] dark:bg-input/30"
                        placeholder={t('editor.outlinePlaceholder')}
                        value={outline}
                        onChange={(e) => onOutlineChange(e.target.value)}
                        disabled={isAnalyzing}
                    />

                    {!hasAgentPlan && (
                        isAnalyzing ? (
                            <div className="flex items-center gap-2">
                                <Button variant="destructive" onClick={onCancel}>
                                    <Square className="h-4 w-4" />
                                    {t('editor.cancelAnalyze')}
                                </Button>
                            </div>
                        ) : (
                            <div className="flex items-center justify-between flex-wrap gap-2">
                                <label className="flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer select-none">
                                    <input
                                        type="checkbox"
                                        checked={enableThinking}
                                        onChange={(e) => onToggleThinking(e.target.checked)}
                                        className="rounded border-border"
                                    />
                                    <BrainCircuit className="h-3.5 w-3.5" />
                                    {t('editor.enableThinking')}
                                </label>
                                <Button onClick={onAnalyze} disabled={!outline.trim()}>
                                    <Sparkles className="h-4 w-4" />
                                    {t('editor.analyze')}
                                </Button>
                            </div>
                        )
                    )}
                    {hasAgentPlan && onCancel && (
                        <Button variant="destructive" size="sm" onClick={onCancel} className="h-7 text-xs">
                            <Square className="h-3 w-3 mr-1" />
                            {t('editor.cancelAnalyze')}
                        </Button>
                    )}
                </div>
            </DialogContent>
        </Dialog>
    );
}
