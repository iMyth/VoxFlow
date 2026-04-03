import { useState } from 'react';
import { GripVertical, Trash2, Volume2, Loader2 } from 'lucide-react';
import { useScriptStore } from '../../store/scriptStore';
import { useCharacterStore } from '../../store/characterStore';
import { useProjectStore } from '../../store/projectStore';
import * as ipc from '../../lib/ipc';
import AudioPlayer from './AudioPlayer';
import type { ScriptLine, AudioFragment } from '../../types';

interface ScriptLineProps {
    line: ScriptLine;
    index: number;
}

export default function ScriptLineComponent({ line, index }: ScriptLineProps) {
    const { updateLine, assignCharacter, deleteLine, reorderLines, setGap } = useScriptStore();
    const { characters } = useCharacterStore();
    const currentProject = useProjectStore((s) => s.currentProject);
    const [generating, setGenerating] = useState(false);
    const [audioFragment, setAudioFragment] = useState<AudioFragment | null>(
        currentProject?.audio_fragments.find((a) => a.line_id === line.id) ?? null,
    );

    const handleGenerateTts = async () => {
        if (!currentProject || !line.text.trim()) return;
        const character = characters.find((c) => c.id === line.character_id);
        setGenerating(true);
        try {
            // Auto-save script before TTS to ensure line_id exists in DB
            const { saveScript } = useScriptStore.getState();
            await saveScript();


            const apiKey = await ipc.loadApiKey('dashscope');
            const fragment = await ipc.generateTts(
                currentProject.project.id,
                line.id,
                line.text,
                {
                    tts_model: character?.tts_model ?? 'qwen3-tts-flash',
                    voice_name: character?.voice_name ?? 'Cherry',
                    speed: character?.speed ?? 1.0,
                    pitch: character?.pitch ?? 1.0,
                },
                apiKey ?? '',
            );
            setAudioFragment(fragment);
            debugger
            // Update audio_fragments in project store so ExportPanel sees it
            const store = useProjectStore.getState();
            if (store.currentProject) {
                const existing = store.currentProject.audio_fragments.filter(
                    (a) => a.line_id !== fragment.line_id,
                );
                useProjectStore.setState({
                    currentProject: {
                        ...store.currentProject,
                        audio_fragments: [...existing, fragment],
                    },
                });
            }
        } catch (e) {
            console.error('TTS generation failed:', e);
        } finally {
            setGenerating(false);
        }
    };

    const handleDragStart = (e: React.DragEvent) => {
        e.dataTransfer.setData('text/plain', String(index));
        e.dataTransfer.effectAllowed = 'move';
    };

    const handleDragOver = (e: React.DragEvent) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
    };

    const handleDrop = (e: React.DragEvent) => {
        e.preventDefault();
        const fromIndex = parseInt(e.dataTransfer.getData('text/plain'), 10);
        if (!isNaN(fromIndex) && fromIndex !== index) {
            reorderLines(fromIndex, index);
        }
    };

    const characterName = characters.find((c) => c.id === line.character_id)?.name;

    return (
        <div
            className="flex items-start gap-2 rounded-xl border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-3 group"
            draggable
            onDragStart={handleDragStart}
            onDragOver={handleDragOver}
            onDrop={handleDrop}
        >
            {/* Drag handle */}
            <div className="cursor-grab pt-2 text-gray-400 hover:text-gray-600">
                <GripVertical className="h-4 w-4" />
            </div>

            {/* Line number */}
            <span className="pt-2 text-xs text-gray-400 w-6 text-right shrink-0">{index + 1}</span>

            {/* Content area */}
            <div className="flex-1 space-y-2">
                <textarea
                    className="w-full rounded-lg border border-gray-200 dark:border-gray-600 px-3 py-2 text-sm dark:bg-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500 resize-y min-h-[40px]"
                    value={line.text}
                    onChange={(e) => updateLine(line.id, e.target.value)}
                    placeholder="输入台词..."
                />
                <div className="flex items-center gap-3">
                    {/* Character dropdown */}
                    <select
                        className="rounded-lg border border-gray-200 dark:border-gray-600 px-2 py-1 text-xs dark:bg-gray-900"
                        value={line.character_id ?? ''}
                        onChange={(e) => assignCharacter(line.id, e.target.value)}
                    >
                        <option value="">未分配角色</option>
                        {characters.map((c) => (
                            <option key={c.id} value={c.id}>{c.name}</option>
                        ))}
                    </select>

                    {characterName && (
                        <span className="text-xs text-blue-500">{characterName}</span>
                    )}

                    {/* TTS status + generate button */}
                    {audioFragment ? (
                        <span className="text-xs text-green-500">✓ 已生成</span>
                    ) : (
                        <span className="text-xs text-gray-400">未生成</span>
                    )}

                    <button
                        className="flex items-center gap-1 rounded-lg bg-teal-600 px-2 py-1 text-xs text-white hover:bg-teal-700 disabled:opacity-50"
                        onClick={handleGenerateTts}
                        disabled={generating || !line.text.trim()}
                    >
                        {generating ? (
                            <Loader2 className="h-3 w-3 animate-spin" />
                        ) : (
                            <Volume2 className="h-3 w-3" />
                        )}
                        {generating ? '生成中' : '生成语音'}
                    </button>

                    {/* Audio player */}
                    {audioFragment && <AudioPlayer filePath={audioFragment.file_path} />}

                    {/* Per-line gap control */}
                    <span className="inline-flex items-center gap-1 text-xs text-gray-400">
                        停顿
                        <input
                            type="number"
                            className="w-14 rounded border border-gray-300 dark:border-gray-600 px-1 py-0.5 text-xs text-center dark:bg-gray-900"
                            value={line.gap_after_ms}
                            onChange={(e) => setGap(line.id, parseInt(e.target.value) || 0)}
                            min={0}
                            max={5000}
                            step={100}
                        />
                        ms
                    </span>
                </div>
            </div>

            {/* Delete button */}
            <button
                className="pt-2 opacity-0 group-hover:opacity-100 transition text-gray-400 hover:text-red-500"
                onClick={() => deleteLine(line.id)}
                aria-label="Delete line"
            >
                <Trash2 className="h-4 w-4" />
            </button>
        </div>
    );
}
