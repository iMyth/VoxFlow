import { useState, useEffect, useRef } from 'react';
import { Sparkles, Save, Plus, Volume2, Loader2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useScriptStore } from '../../store/scriptStore';
import { useProjectStore } from '../../store/projectStore';
import ScriptLineComponent from './ScriptLine';
import { Button } from '../ui/button';
import { Card, CardContent } from '../ui/card';
import { Label } from '../ui/label';
import { Progress } from '../ui/progress';

export default function ScriptEditor() {
    const { t } = useTranslation();
    const {
        lines, isGenerating, isDirty, streamingText,
        generateScript, saveScript, addLine,
        isBatchTtsRunning, batchTtsProgress, generateAllTts,
    } = useScriptStore();
    const currentProject = useProjectStore((s) => s.currentProject);
    const [outline, setOutline] = useState('');
    const outlineRef = useRef(outline);

    // Sync outline from project data
    useEffect(() => {
        const projectOutline = currentProject?.project.outline ?? '';
        setOutline(projectOutline);
    }, [currentProject?.project.id, currentProject?.project.outline]);

    outlineRef.current = outline;

    // ---- Auto-save outline: debounce 3 seconds ----
    const outlineSaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    useEffect(() => {
        const projectId = currentProject?.project.id;
        if (!projectId) return;

        if (outlineSaveTimerRef.current) {
            clearTimeout(outlineSaveTimerRef.current);
        }
        outlineSaveTimerRef.current = setTimeout(async () => {
            await useProjectStore.getState().saveOutline(outlineRef.current);
        }, 3000);

        return () => {
            if (outlineSaveTimerRef.current) {
                clearTimeout(outlineSaveTimerRef.current);
            }
        };
    }, [outline, currentProject?.project.id]);

    // ---- Auto-save script: debounce 5 seconds ----
    const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    useEffect(() => {
        if (isDirty) {
            if (saveTimerRef.current) {
                clearTimeout(saveTimerRef.current);
            }
            saveTimerRef.current = setTimeout(() => {
                saveScript();
            }, 5000);
        }

        return () => {
            if (saveTimerRef.current) {
                clearTimeout(saveTimerRef.current);
            }
        };
    }, [isDirty, lines, saveScript]);

    // ---- Count missing TTS lines ----
    const audioFragments = currentProject?.audio_fragments ?? [];
    const coveredLineIds = new Set(audioFragments.map((a) => a.line_id));
    const missingTtsCount = lines.filter(
        (l) => l.text.trim() && !coveredLineIds.has(l.id),
    ).length;

    const handleGenerate = () => {
        if (!outline.trim() || isGenerating) return;
        generateScript(outline.trim());
    };

    return (
        <div className="mx-auto max-w-4xl px-6 py-6 space-y-4 relative">
            {/* Blocking overlay during generation */}
            {isGenerating && (
                <div className="absolute inset-0 z-50 bg-background/60 backdrop-blur-sm flex items-center justify-center rounded-xl">
                    <div className="flex flex-col items-center gap-3">
                        <Loader2 className="h-10 w-10 animate-spin text-primary" />
                        <p className="text-sm font-medium text-muted-foreground">{t('editor.generating')}</p>
                        <p className="text-xs text-muted-foreground">{t('editor.generatingHint')}</p>
                    </div>
                </div>
            )}
            <Card>
                <CardContent className="space-y-3">
                    <Label>{t('editor.outlineLabel')}</Label>
                    <textarea
                        className="w-full rounded-lg border border-input bg-transparent px-3 py-2 text-sm focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 outline-none resize-y min-h-[80px] dark:bg-input/30"
                        placeholder={t('editor.outlinePlaceholder')}
                        value={outline}
                        onChange={(e) => setOutline(e.target.value)}
                        disabled={isGenerating}
                    />
                    <div className="flex gap-2 flex-wrap items-center">
                        <Button onClick={handleGenerate} disabled={isGenerating || !outline.trim()}>
                            <Sparkles className="h-4 w-4" />
                            {isGenerating ? t('editor.generating') : t('editor.generate')}
                        </Button>
                        {isDirty && (
                            <Button variant="outline" onClick={() => saveScript()}>
                                <Save className="h-4 w-4" /> {t('editor.save')}
                            </Button>
                        )}
                        {missingTtsCount > 0 && (
                            <Button
                                variant="outline"
                                onClick={() => generateAllTts()}
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
                </CardContent>
            </Card>

            {isGenerating && streamingText && (
                <Card className="border-purple-200 dark:border-purple-800 bg-purple-50 dark:bg-purple-900/20">
                    <CardContent>
                        <p className="text-sm text-purple-700 dark:text-purple-300 font-medium mb-2">{t('editor.aiGenerating')}</p>
                        <pre className="text-sm whitespace-pre-wrap">{streamingText}<span className="animate-pulse">▌</span></pre>
                    </CardContent>
                </Card>
            )}

            {lines.length > 0 && (
                <div className="space-y-2">
                    {lines.map((line, index) => (
                        <ScriptLineComponent key={line.id} line={line} index={index} />
                    ))}
                    <Button variant="outline" className="w-full border-dashed" onClick={() => addLine(lines.length - 1)}>
                        <Plus className="h-4 w-4" /> {t('editor.addLine')}
                    </Button>
                </div>
            )}

            {lines.length === 0 && !isGenerating && (
                <p className="text-center text-muted-foreground py-16">{t('editor.emptyHint')}</p>
            )}

            {lines.length === 0 && !isGenerating && (
                <div className="flex justify-center">
                    <Button variant="outline" className="border-dashed" onClick={() => addLine(-1)}>
                        <Plus className="h-4 w-4" /> {t('editor.addFirstLine')}
                    </Button>
                </div>
            )}
        </div>
    );
}
