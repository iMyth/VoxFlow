import { useState } from 'react';
import { Plus, Trash2, GripVertical, ChevronDown, ChevronRight } from 'lucide-react';
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
    const { deleteSection, renameSection, reorderSections } = useScriptStore();
    const [editing, setEditing] = useState(false);
    const [title, setTitle] = useState(section.title);
    const [collapsed, setCollapsed] = useState(false);

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

    const handleGripDragStart = (e: React.DragEvent) => {
        e.dataTransfer.setData('section-index', String(index));
        e.dataTransfer.effectAllowed = 'move';
    };

    const handleSectionDragOver = (e: React.DragEvent) => {
        // Only accept section drags, let line drags pass through
        if (e.dataTransfer.getData('section-index') !== '') {
            e.preventDefault();
            e.dataTransfer.dropEffect = 'move';
        }
    };

    const handleSectionDrop = (e: React.DragEvent) => {
        const fromIndex = parseInt(e.dataTransfer.getData('section-index'), 10);
        if (!isNaN(fromIndex) && fromIndex !== index) {
            reorderSections(fromIndex, index);
        }
    };

    const handleLineDragOver = (e: React.DragEvent) => {
        // Stop section drags from reaching this container (line drags handled by children)
        if (e.dataTransfer.getData('section-index') !== '') {
            e.stopPropagation();
            e.preventDefault();
        }
    };

    const handleLineDrop = (e: React.DragEvent) => {
        // Prevent section drops on line cards
        if (e.dataTransfer.getData('section-index') !== '') {
            e.stopPropagation();
        }
    };

    return (
        <div
            className="group/section space-y-2"
            onDragOver={handleSectionDragOver}
            onDrop={handleSectionDrop}
        >
            {/* Section header */}
            <div className="flex items-center gap-2 px-1">
                <div
                    className="cursor-grab select-none text-muted-foreground hover:text-foreground shrink-0"
                    draggable
                    onDragStart={handleGripDragStart}
                >
                    <GripVertical className="h-4 w-4" />
                </div>

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

                <div className="flex items-center gap-1 opacity-0 group-hover/section:opacity-100 transition-opacity">
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
                <div
                    className="space-y-2"
                    onDragOver={handleLineDragOver}
                    onDrop={handleLineDrop}
                >
                    {lines.map((line, lineIndex) => (
                        <ScriptLineComponent key={line.id} line={line} index={lineIndex} />
                    ))}
                    <Button variant="outline" className="w-full border-dashed" onClick={onAddLine}>
                        <Plus className="h-4 w-4" /> {t('editor.addLine')}
                    </Button>
                </div>
            )}
        </div>
    );
}
