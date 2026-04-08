import { Plus, Undo2, Redo2, Wand2, X } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Separator } from '../ui/separator';
import ScriptLineComponent from './ScriptLine';
import SectionGroup from './SectionGroup';
import type { ScriptLine, ScriptSection } from '../../types';
import { useScriptStore } from '../../store/scriptStore';

interface ScriptLinesProps {
    lines: ScriptLine[];
    sections: ScriptSection[];
    emptyHint: string;
    emptyActionLabel: string;
    onEmptyAction: () => void;
}

export default function ScriptLines({
    lines,
    sections,
    emptyHint,
    emptyActionLabel,
    onEmptyAction,
}: ScriptLinesProps) {
    const { t } = useTranslation();
    const { addLine, addSection, setAllInstructions } = useScriptStore();
    const [batchInstructionsOpen, setBatchInstructionsOpen] = useState(false);
    const [batchInstructionsValue, setBatchInstructionsValue] = useState('');

    const handleBatchInstructions = () => {
        if (batchInstructionsValue.trim()) {
            setAllInstructions(batchInstructionsValue.trim());
            setBatchInstructionsOpen(false);
            setBatchInstructionsValue('');
        }
    };

    const handleBatchKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Enter') {
            handleBatchInstructions();
        } else if (e.key === 'Escape') {
            setBatchInstructionsOpen(false);
            setBatchInstructionsValue('');
        }
    };

    const Toolbar = (
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
            <Button
                variant="ghost"
                size="sm"
                className={`h-6 w-6 p-0 ${batchInstructionsOpen ? 'text-purple-500' : ''}`}
                onClick={() => {
                    if (batchInstructionsOpen) {
                        setBatchInstructionsOpen(false);
                        setBatchInstructionsValue('');
                    } else {
                        setBatchInstructionsOpen(true);
                    }
                }}
                title={t('editor.setAllInstructions')}
            >
                {batchInstructionsOpen ? <X className="h-3.5 w-3.5" /> : <Wand2 className="h-3.5 w-3.5" />}
            </Button>
            {batchInstructionsOpen && (
                <div className="flex items-center gap-2 flex-1 ml-2">
                    <Input
                        value={batchInstructionsValue}
                        onChange={(e) => setBatchInstructionsValue(e.target.value)}
                        onKeyDown={handleBatchKeyDown}
                        className="h-7 text-xs flex-1 border-purple-300/50 focus-visible:border-purple-500"
                        placeholder={t('editor.instructionsPlaceholder')}
                        autoFocus
                    />
                    <Button
                        size="sm"
                        className="h-7 text-xs"
                        onClick={handleBatchInstructions}
                        disabled={!batchInstructionsValue.trim()}
                    >
                        {t('editor.setAllInstructions')}
                    </Button>
                </div>
            )}
        </div>
    );

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

    const sortedSections = [...sections].sort((a, b) => a.section_order - b.section_order);

    // If we have sections, render section groups
    if (sortedSections.length > 0) {
        // Build a map of section_id -> lines
        const linesBySection = new Map<string, ScriptLine[]>();
        const unassignedLines: ScriptLine[] = [];

        for (const line of lines) {
            if (line.section_id && linesBySection.has(line.section_id)) {
                linesBySection.get(line.section_id)!.push(line);
            } else if (line.section_id) {
                linesBySection.set(line.section_id, [line]);
            } else {
                unassignedLines.push(line);
            }
        }

        return (
            <>
                <Separator className="border-dashed" />
                {Toolbar}
                <div className="space-y-4">
                    {sortedSections.map((section, index) => (
                        <SectionGroup
                            key={section.id}
                            section={section}
                            lines={linesBySection.get(section.id) ?? []}
                            index={index}
                            totalSections={sortedSections.length}
                            onAddLine={() => addLine(-1, section.id)}
                        />
                    ))}
                    {/* Unassigned lines */}
                    {unassignedLines.length > 0 && (
                        <div className="space-y-2">
                            {unassignedLines.map((line, index) => (
                                <ScriptLineComponent key={line.id} line={line} index={index} />
                            ))}
                        </div>
                    )}
                    <Button variant="outline" className="w-full border-dashed" onClick={addSection}>
                        <Plus className="h-4 w-4" /> {t('editor.addSection')}
                    </Button>
                </div>
            </>
        );
    }

    // No sections: flat list (backward compatible, fallback section "正文")
    return (
        <>
            <Separator className="border-dashed" />
            {Toolbar}
            <div className="space-y-2">
                {lines.map((line, index) => (
                    <ScriptLineComponent key={line.id} line={line} index={index} />
                ))}
                <Button variant="outline" className="w-full border-dashed" onClick={() => addLine(lines.length - 1)}>
                    <Plus className="h-4 w-4" /> {t('editor.addLine')}
                </Button>
            </div>
        </>
    );
}
