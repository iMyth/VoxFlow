import { useState, useCallback } from 'react';
import { Plus, Trash2, ChevronDown, ChevronRight } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import ScriptLineComponent from './ScriptLine';
import { useScriptStore } from '../../store/scriptStore';
import type { ScriptSection, ScriptLine } from '../../types';

interface SectionGroupProps {
    section: ScriptSection;
    lines: ScriptLine[];
    index: number;
    totalSections: number;
    onAddLine: () => void;
}

export default function SectionGroup({
    section,
    lines,
    index,
    totalSections,
    onAddLine,
}: SectionGroupProps) {
    const { t } = useTranslation();
    const { deleteSection, renameSection } = useScriptStore();
    const [editing, setEditing] = useState(false);
    const [title, setTitle] = useState(section.title);
    const [collapsed, setCollapsed] = useState(false);

    // Drag state (lifted up so parent can manage all children's visuals)
    const [draggingId, setDraggingId] = useState<string | null>(null);
    const [dropTarget, setDropTarget] = useState<{ id: string; position: 'before' | 'after' } | null>(null);

    const handleTitleBlur = () => {
        setEditing(false);
        if (title.trim() && title !== section.title) {
            renameSection(section.id, title.trim());
        } else {
            setTitle(section.title);
        }
    };

    const handleTitleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Enter') {
            (e.target as HTMLInputElement).blur();
        }
    };

    const canDelete = lines.every((l) => !l.text.trim());

    /* ---- Line drag: pointer-based (works reliably in Tauri) ---- */
    const handleDragStart = useCallback((lineId: string, _pointerId: number) => {
        setDraggingId(lineId);
        setDropTarget(null);
    }, []);

    const handleDragMove = useCallback((clientX: number, clientY: number) => {
        if (!draggingId) return;
        const el = document.elementFromPoint(clientX, clientY);
        const card = el?.closest('[data-line-id]') as HTMLElement | null;
        if (!card) { setDropTarget(null); return; }
        const targetId = card.getAttribute('data-line-id');
        if (!targetId || targetId === draggingId) { setDropTarget(null); return; }
        const rect = card.getBoundingClientRect();
        const position: 'before' | 'after' = clientY < rect.top + rect.height / 2 ? 'before' : 'after';
        setDropTarget(prev =>
            prev?.id === targetId && prev?.position === position ? prev : { id: targetId, position },
        );
    }, [draggingId]);

    const handleDragEnd = useCallback(() => {
        if (draggingId && dropTarget && draggingId !== dropTarget.id) {
            const allLines = useScriptStore.getState().lines;
            const fromIdx = allLines.findIndex((l) => l.id === draggingId);
            const targetIdx = allLines.findIndex((l) => l.id === dropTarget.id);
            if (fromIdx !== -1 && targetIdx !== -1) {
                let toIdx = dropTarget.position === 'before' ? targetIdx : targetIdx + 1;
                if (fromIdx < toIdx) toIdx -= 1;
                if (fromIdx !== toIdx) useScriptStore.getState().reorderLines(fromIdx, toIdx);
            }
        }
        setDraggingId(null);
        setDropTarget(null);
    }, [draggingId, dropTarget]);

    return (
        <div
            className="group/section space-y-2"
            data-section-index={index}
        >
            {/* Section header */}
            <div
                className="flex items-center gap-2 px-1"
            >
                <button
                    className="text-muted-foreground hover:text-foreground transition-colors shrink-0"
                    onClick={() => setCollapsed(!collapsed)}
                >
                    {collapsed ? (
                        <ChevronRight className="h-4 w-4" />
                    ) : (
                        <ChevronDown className="h-4 w-4" />
                    )}
                </button>

                {editing ? (
                    <Input
                        value={title}
                        onChange={(e) => setTitle(e.target.value)}
                        onBlur={handleTitleBlur}
                        onKeyDown={handleTitleKeyDown}
                        className="h-7 text-sm font-semibold max-w-[200px]"
                        autoFocus
                    />
                ) : (
                    <h3
                        className="text-sm font-semibold text-foreground cursor-pointer hover:text-muted-foreground transition-colors flex-1"
                        onClick={() => setEditing(true)}
                    >
                        {section.title}
                    </h3>
                )}

                <div
                    className="flex items-center gap-1 opacity-0 group-hover/section:opacity-100 transition-opacity"
                >
                    {canDelete && totalSections > 1 && (
                        <Button
                            variant="ghost"
                            size="icon-sm"
                            className="h-6 w-6 p-0 text-muted-foreground hover:text-destructive"
                            onClick={() => deleteSection(section.id)}
                            title={t('editor.deleteSection')}
                        >
                            <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                    )}
                </div>
            </div>

            {/* Lines */}
            {!collapsed && (
                <div className="space-y-2">
                    {lines.map((line, lineIndex) => (
                        <ScriptLineComponent
                            key={line.id}
                            line={line}
                            index={lineIndex}
                            totalLines={lines.length}
                            isDragging={draggingId === line.id}
                            dropPosition={dropTarget?.id === line.id ? dropTarget.position : null}
                            onDragStart={handleDragStart}
                            onDragMove={handleDragMove}
                            onDragEnd={handleDragEnd}
                        />
                    ))}
                    <Button variant="outline" className="w-full border-dashed" onClick={onAddLine}>
                        <Plus className="h-4 w-4" /> {t('editor.addLine')}
                    </Button>
                </div>
            )}
        </div>
    );
}
