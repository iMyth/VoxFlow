import { Plus, Undo2, Redo2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import { Separator } from '../ui/separator';
import ScriptLineComponent from './ScriptLine';
import type { ScriptLine } from '../../types';
import { useScriptStore } from '../../store/scriptStore';

interface ScriptLinesProps {
    lines: ScriptLine[];
    emptyHint: string;
    emptyActionLabel: string;
    onEmptyAction: () => void;
}

export default function ScriptLines({
    lines,
    emptyHint,
    emptyActionLabel,
    onEmptyAction,
}: ScriptLinesProps) {
    const { t } = useTranslation();

    if (lines.length === 0) {
        return (
            <div className="flex flex-col items-center justify-center py-16 gap-4">
                <p className="text-center text-muted-foreground">{emptyHint}</p>
                <Button variant="outline" className="border-dashed" onClick={onEmptyAction}>
                    <Plus className="h-4 w-4" /> {emptyActionLabel}
                </Button>
            </div>
        );
    }

    return (
        <>
            <Separator className="border-dashed" />
            <div className="flex items-center gap-1 mb-1">
                <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0"
                    onClick={() => useScriptStore.temporal.getState().undo()}
                    title={`${t('editor.undo')} (⌘Z)`}
                >
                    <Undo2 className="h-3.5 w-3.5" />
                </Button>
                <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 w-6 p-0"
                    onClick={() => useScriptStore.temporal.getState().redo()}
                    title={`${t('editor.redo')} (⇧⌘Z)`}
                >
                    <Redo2 className="h-3.5 w-3.5" />
                </Button>
            </div>
            <div className="space-y-2">
                {lines.map((line, index) => (
                    <ScriptLineComponent key={line.id} line={line} index={index} />
                ))}
                <Button variant="outline" className="w-full border-dashed" onClick={() => useScriptStore.getState().addLine(lines.length - 1)}>
                    <Plus className="h-4 w-4" /> {t('editor.addLine')}
                </Button>
            </div>
        </>
    );
}
