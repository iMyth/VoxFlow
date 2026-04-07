import { useState, useEffect } from 'react';
import { GripVertical, Trash2, Volume2, Loader2, AlertCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useScriptStore } from '../../store/scriptStore';
import { useCharacterStore } from '../../store/characterStore';
import { useProjectStore } from '../../store/projectStore';
import { useToastStore } from '../../store/toastStore';
import * as ipc from '../../lib/ipc';
import AudioPlayer from './AudioPlayer';
import { Button } from '../ui/button';
import { Badge } from '../ui/badge';
import { Input } from '../ui/input';
import { Card } from '../ui/card';
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from '../ui/select';
import type { ScriptLine, AudioFragment } from '../../types';

interface ScriptLineProps {
    line: ScriptLine;
    index: number;
}

export default function ScriptLineComponent({ line, index }: ScriptLineProps) {
    const { t } = useTranslation();
    const { updateLine, assignCharacter, deleteLine, reorderLines, setGap } = useScriptStore();
    const { characters } = useCharacterStore();
    const currentProject = useProjectStore((s) => s.currentProject);
    const [generating, setGenerating] = useState(false);
    const [ttsError, setTtsError] = useState<string | null>(null);
    const [audioFragment, setAudioFragment] = useState<AudioFragment | null>(
        currentProject?.audio_fragments.find((a) => a.line_id === line.id) ?? null,
    );

    // Sync local audioFragment with project store (e.g. after batch TTS)
    useEffect(() => {
        const frag = currentProject?.audio_fragments.find((a) => a.line_id === line.id) ?? null;
        setAudioFragment(frag);
    }, [line.id, currentProject?.audio_fragments]);

    // Clear error when audio fragment appears
    useEffect(() => {
        if (audioFragment) setTtsError(null);
    }, [audioFragment]);

    const handleGenerateTts = async () => {
        if (!currentProject || !line.text.trim()) return;
        const character = characters.find((c) => c.id === line.character_id);
        setGenerating(true);
        setTtsError(null);
        try {
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
            const msg = String(e);
            setTtsError(msg.length > 100 ? msg.slice(0, 100) + '...' : msg);
            useToastStore.getState().addToast(`第 ${index + 1} 行语音生成失败`);
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
    const UNASSIGNED = '__unassigned__';

    return (
        <Card
            className="flex-row items-start gap-2 p-3 group"
            draggable
            onDragStart={handleDragStart}
            onDragOver={handleDragOver}
            onDrop={handleDrop}
        >
            <div className="cursor-grab pt-2 text-muted-foreground hover:text-foreground">
                <GripVertical className="h-4 w-4" />
            </div>

            <span className="pt-2 text-xs text-muted-foreground w-6 text-right shrink-0">{index + 1}</span>

            <div className="flex-1 space-y-2">
                <textarea
                    className="w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 outline-none resize-y min-h-[40px] dark:bg-input/30"
                    value={line.text}
                    onChange={(e) => updateLine(line.id, e.target.value)}
                    placeholder={t('editor.linePlaceholder')}
                />
                <div className="flex items-center gap-3 flex-wrap">
                    <Select
                        value={line.character_id ?? UNASSIGNED}
                        onValueChange={(v) => assignCharacter(line.id, v === UNASSIGNED ? '' : v)}
                    >
                        <SelectTrigger size="sm">
                            <SelectValue placeholder={t('editor.unassigned')} />
                        </SelectTrigger>
                        <SelectContent>
                            <SelectItem value={UNASSIGNED}>{t('editor.unassigned')}</SelectItem>
                            {characters.map((c) => (
                                <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>
                            ))}
                        </SelectContent>
                    </Select>

                    {characterName && (
                        <Badge variant="secondary">{characterName}</Badge>
                    )}

                    {ttsError && (
                        <Badge variant="destructive" className="gap-1">
                            <AlertCircle className="h-3 w-3" /> 生成失败
                        </Badge>
                    )}

                    {audioFragment ? (
                        <Badge variant="outline" className="text-green-600 border-green-300 gap-1">
                            {t('editor.generated')}
                            {audioFragment.duration_ms != null && (
                                <span className="text-xs opacity-70">
                                    ({(audioFragment.duration_ms / 1000).toFixed(1)}s)
                                </span>
                            )}
                        </Badge>
                    ) : !ttsError ? (
                        <Badge variant="outline" className="text-muted-foreground">{t('editor.notGenerated')}</Badge>
                    ) : null}

                    <Button
                        size="xs"
                        onClick={handleGenerateTts}
                        disabled={generating || !line.text.trim()}
                    >
                        {generating ? (
                            <Loader2 className="h-3 w-3 animate-spin" />
                        ) : (
                            <Volume2 className="h-3 w-3" />
                        )}
                        {generating ? t('editor.generatingTts') : t('editor.generateTts')}
                    </Button>

                    {audioFragment && <AudioPlayer filePath={audioFragment.file_path} />}

                    <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                        {t('editor.gap')}
                        <Input
                            type="number"
                            className="w-24 h-6 text-xs text-center"
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

            <Button
                variant="ghost"
                size="icon-sm"
                className="pt-2 opacity-0 group-hover:opacity-100 transition hover:text-destructive"
                onClick={() => deleteLine(line.id)}
                aria-label="Delete line"
            >
                <Trash2 className="h-4 w-4" />
            </Button>
        </Card>
    );
}
