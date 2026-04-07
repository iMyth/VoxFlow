import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useScriptStore } from '../../store/scriptStore';
import { useProjectStore } from '../../store/projectStore';
import { useCharacterStore } from '../../store/characterStore';
import { useToastStore } from '../../store/toastStore';
import { useSettingsStore } from '../../store/settingsStore';
import ModeSelector from './ModeSelector';
import OutlinePanel from './OutlinePanel';
import PlanCard from './PlanCard';
import ScriptLines from './ScriptLines';
import StreamingCard from './StreamingCard';
import ActionBar from './ActionBar';
import ThinkingPanel from './ThinkingPanel';
import ConfirmDialog from '../ui/confirm-dialog';

export default function ScriptEditor() {
    const { t } = useTranslation();
    const {
        lines, sections, isGenerating, isAnalyzing, isDirty, streamingText, thinkingText,
        enableThinking, setEnableThinking,
        agentPlan, workflow, setWorkflow, analyzeOutline, setAgentPlan, generateScript,
        cancelLlm, saveScript, generateAllTts,
        isBatchTtsRunning, batchTtsProgress,
    } = useScriptStore();
    const currentProject = useProjectStore((s) => s.currentProject);
    const existingCharacters = useCharacterStore((s) => s.characters);
    const [outline, setOutline] = useState('');
    const [showOverwriteConfirm, setShowOverwriteConfirm] = useState(false);
    const [showAnalyzeConfirm, setShowAnalyzeConfirm] = useState(false);
    const [confirmData, setConfirmData] = useState({ textCount: 0, audioCount: 0 });
    const [extraInstructions, setExtraInstructions] = useState('');
    const [characterMapping, setCharacterMapping] = useState<Record<string, string>>({});
    const [creatingChars, setCreatingChars] = useState<Record<string, boolean>>({});
    const [newCharForms, setNewCharForms] = useState<Record<string, { name: string; voice: string; speed: number; pitch: number }>>({});
    const outlineRef = useRef(outline);

    // Auto-populate character mapping when plan arrives with matched characters
    useEffect(() => {
        if (agentPlan) {
            const autoMapping: Record<string, string> = {};
            agentPlan.suggested_characters.forEach((ch) => {
                if (ch.matched_existing && ch.existing_id) {
                    // characterMapping stores character names (used by SelectItem values),
                    // but existing_id is a UUID — look up the name
                    const matchedChar = existingCharacters.find((ec) => ec.id === ch.existing_id);
                    if (matchedChar) {
                        autoMapping[ch.name] = matchedChar.name;
                    }
                }
            });
            setCharacterMapping(autoMapping);
        }
    }, [agentPlan, existingCharacters]);

    // Sync outline from project data and restore workflow on re-entry
    useEffect(() => {
        const projectOutline = currentProject?.project.outline ?? '';
        setOutline(projectOutline);

        if (currentProject?.project.id) {
            const { workflow: currentWorkflow } = useScriptStore.getState();
            if (currentWorkflow === null) {
                // Restore based on project state: outline present → AI, existing lines → manual
                if (projectOutline) {
                    useScriptStore.getState().setWorkflow('ai');
                } else if (useScriptStore.getState().lines.length > 0) {
                    useScriptStore.getState().setWorkflow('manual');
                }
            }
        }
    }, [currentProject?.project.id]);

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

    // ---- Mode selection ----
    const handleAiModeSelect = () => {
        setWorkflow('ai');
    };

    const handleManualModeSelect = () => {
        setWorkflow('manual');
    };

    // ---- Outline actions ----
    const handleAnalyze = () => {
        if (!outline.trim() || isAnalyzing || isGenerating) return;

        const existingText = lines.filter((l) => l.text.trim()).length;
        if (existingText > 0) {
            setShowAnalyzeConfirm(true);
            return;
        }

        analyzeOutline(outline.trim());
    };

    // ---- Plan actions ----
    const handleCharacterMapping = (suggestedName: string, targetId: string) => {
        setCharacterMapping((prev) => ({ ...prev, [suggestedName]: targetId }));
        if (targetId === '__new__') {
            setCreatingChars((prev) => ({ ...prev, [suggestedName]: true }));
            setNewCharForms((prev) => {
                if (prev[suggestedName]) return prev;
                const defaults = useSettingsStore.getState();
                return { ...prev, [suggestedName]: { name: suggestedName, voice: defaults.defaultVoiceName, speed: defaults.defaultSpeed, pitch: defaults.defaultPitch } };
            });
        } else {
            setCreatingChars((prev) => ({ ...prev, [suggestedName]: false }));
        }
    };

    const handleFormChange = (name: string, field: string, value: string | number) => {
        setNewCharForms((prev) => ({
            ...prev,
            [name]: { ...prev[name], [field]: value },
        }));
    };

    const handleCreateNewChar = async (suggestedName: string) => {
        const form = newCharForms[suggestedName];
        if (!form || !form.name.trim()) {
            useToastStore.getState().addToast('editor.charNameRequired');
            return;
        }
        await useCharacterStore.getState().createCharacter({
            name: form.name.trim(),
            tts_model: '',
            voice_name: form.voice,
            speed: form.speed,
            pitch: form.pitch,
        });
        await useCharacterStore.getState().fetchCharacters();
        setCharacterMapping((prev) => ({ ...prev, [suggestedName]: form.name.trim() }));
        setCreatingChars((prev) => ({ ...prev, [suggestedName]: false }));
        useToastStore.getState().addToast(t('editor.charCreated', { name: form.name }));
    };

    const handleCancelNewChar = (suggestedName: string) => {
        setCreatingChars((prev) => ({ ...prev, [suggestedName]: false }));
        setCharacterMapping((prev) => ({ ...prev, [suggestedName]: '' }));
    };

    const handleConfirmGenerate = () => {
        if (!agentPlan) return;
        const unmapped = agentPlan.suggested_characters.filter(
            (c) => !characterMapping[c.name],
        );
        if (unmapped.length > 0) {
            useToastStore.getState().addToast('editor.mapAllCharsRequired');
            return;
        }
        const pendingCreate = agentPlan.suggested_characters.filter(
            (c) => creatingChars[c.name],
        );
        if (pendingCreate.length > 0) {
            const names = pendingCreate.map(c => c.name).join(', ');
            useToastStore.getState().addToast(t('editor.finishCreatingChars', { names }));
            return;
        }

        const mappingText = Object.entries(characterMapping)
            .map(([suggested, target]) => `  "${suggested}" -> "${target}"`)
            .join('\n');
        const fullInstructions = [
            extraInstructions.trim(),
            `\n角色映射：\n${mappingText}`,
        ].filter(Boolean).join('\n');

        const confirmedCharNames = Object.values(characterMapping);

        const existingText = lines.filter((l) => l.text.trim()).length;
        const existingAudio = coveredLineIds.size;
        if (existingText > 0 || existingAudio > 0) {
            setConfirmData({ textCount: existingText, audioCount: existingAudio });
            setShowOverwriteConfirm(true);
            const w = window as Window & { _pendingExtraInstructions?: string; _pendingCharNames?: string[] };
            w._pendingExtraInstructions = fullInstructions;
            w._pendingCharNames = confirmedCharNames;
            return;
        }

        generateScript(outline.trim(), fullInstructions, confirmedCharNames);
        setAgentPlan(null);
    };

    const handlePlanManualMode = () => {
        setAgentPlan(null);
        setWorkflow('manual');
    };

    const isAiMode = workflow === 'ai';
    const hasNoWorkflow = !workflow;
    const hasLines = lines.length > 0;

    return (
        <div className="mx-auto max-w-4xl px-6 py-6 space-y-4 relative">
            {/* Mode selection - shown on project entry */}
            {hasNoWorkflow && !hasLines && (
                <div className="card p-6 space-y-4">
                    <h2 className="text-lg font-semibold text-center">{t('editor.chooseMode')}</h2>
                    <ModeSelector
                        onSelectAi={handleAiModeSelect}
                        onSelectManual={handleManualModeSelect}
                    />
                </div>
            )}

            {/* Batch TTS blocking overlay */}
            {isBatchTtsRunning && (
                <div className="fixed z-50 flex items-center justify-center bg-background/60 backdrop-blur-sm pointer-events-auto" style={{ top: 0, right: 0, bottom: 0, left: 0, width: '100vw', height: '100vh' }}>
                    <div className="card p-6 space-y-3 text-center max-w-sm">
                        <p className="text-sm font-medium">{t('editor.batchTtsRunning', { current: batchTtsProgress?.current ?? 0, total: batchTtsProgress?.total ?? 0 })}</p>
                        {batchTtsProgress && batchTtsProgress.total > 0 && (
                            <div className="space-y-1">
                                <div className="w-full bg-secondary rounded-full h-2 overflow-hidden">
                                    <div
                                        className="h-full bg-primary rounded-full transition-all duration-300"
                                        style={{ width: `${(batchTtsProgress.current / batchTtsProgress.total) * 100}%` }}
                                    />
                                </div>
                                <p className="text-xs text-muted-foreground">{batchTtsProgress.current} / {batchTtsProgress.total}</p>
                            </div>
                        )}
                        <p className="text-xs text-muted-foreground">{t('editor.batchTtsHint')}</p>
                    </div>
                </div>
            )}

            {/* Switch mode button */}
            {workflow && (
                <div className="flex justify-end">
                    <button
                        className="text-xs text-muted-foreground hover:text-foreground transition-colors"
                        onClick={() => {
                            setWorkflow(null);
                            setAgentPlan(null);
                        }}
                    >
                        {t('editor.switchMode')}
                    </button>
                </div>
            )}

            {/* Outline section - AI mode only */}
            {isAiMode && (
                <OutlinePanel
                    outline={outline}
                    onOutlineChange={setOutline}
                    isAnalyzing={isAnalyzing}
                    enableThinking={enableThinking}
                    onToggleThinking={setEnableThinking}
                    onAnalyze={handleAnalyze}
                    onCancel={cancelLlm}
                    hasAgentPlan={!!agentPlan}
                />
            )}

            {/* Action bar - shown when lines exist and no active plan */}
            {workflow && hasLines && !agentPlan && (
                <ActionBar
                    isDirty={isDirty}
                    isBatchTtsRunning={isBatchTtsRunning}
                    batchTtsProgress={batchTtsProgress}
                    missingTtsCount={missingTtsCount}
                    onSave={saveScript}
                    onGenerateAllTts={generateAllTts}
                />
            )}

            {/* Analyzing streaming */}
            {isAnalyzing && (
                <StreamingCard
                    color="blue"
                    label={t('editor.analyzing')}
                    text={streamingText}
                    onCancel={cancelLlm}
                />
            )}

            {/* Generating streaming */}
            {isGenerating && streamingText && (
                <StreamingCard
                    color="purple"
                    label={t('editor.aiGenerating')}
                    text={streamingText}
                    onCancel={cancelLlm}
                />
            )}

            {/* Thinking panel - shown during analysis or generation when thinking content is present */}
            {(isAnalyzing || isGenerating) && thinkingText && (
                <ThinkingPanel
                    thinkingText={thinkingText}
                    isThinking={isAnalyzing || isGenerating}
                />
            )}

            {/* Plan card - AI mode only */}
            {isAiMode && agentPlan && (
                <PlanCard
                    plan={agentPlan}
                    existingCharacters={existingCharacters}
                    currentProjectId={currentProject?.project.id ?? ''}
                    onDismiss={() => setAgentPlan(null)}
                    onConfirmGenerate={handleConfirmGenerate}
                    onManualMode={handlePlanManualMode}
                    onCharacterMapping={handleCharacterMapping}
                    onNewChar={handleCreateNewChar}
                    onCancelNewChar={handleCancelNewChar}
                    creatingChars={creatingChars}
                    newCharForms={newCharForms}
                    onFormChange={handleFormChange}
                    characterMapping={characterMapping}
                    extraInstructions={extraInstructions}
                    onExtraChange={setExtraInstructions}
                    isGenerating={isGenerating}
                />
            )}

            {/* Script lines */}
            <ScriptLines
                lines={lines}
                sections={sections}
                emptyHint={t('editor.emptyHint')}
                emptyActionLabel={t('editor.addFirstLine')}
                onEmptyAction={() => useScriptStore.getState().addLine(-1)}
            />

            {/* Confirm dialogs */}
            <ConfirmDialog
                open={showOverwriteConfirm}
                onOpenChange={setShowOverwriteConfirm}
                title={t('editor.confirmOverwriteTitle')}
                description={t('editor.confirmOverwrite', {
                    textCount: confirmData.textCount,
                    audioCount: confirmData.audioCount,
                })}
                confirmText={t('editor.confirmOverwriteBtn')}
                cancelText={t('editor.cancel')}
                irreversibleWarning={t('editor.irreversibleWarning')}
                onConfirm={() => {
                    const w = window as Window & { _pendingExtraInstructions?: string; _pendingCharNames?: string[] };
                    if (w._pendingExtraInstructions) {
                        generateScript(outline.trim(), w._pendingExtraInstructions, w._pendingCharNames);
                        setAgentPlan(null);
                        delete w._pendingExtraInstructions;
                        delete w._pendingCharNames;
                    } else {
                        generateScript(outline.trim());
                    }
                }}
            />

            <ConfirmDialog
                open={showAnalyzeConfirm}
                onOpenChange={setShowAnalyzeConfirm}
                title={t('editor.confirmAnalyzeTitle')}
                description={t('editor.confirmAnalyze', { textCount: lines.filter((l) => l.text.trim()).length })}
                confirmText={t('editor.confirmAnalyzeBtn')}
                cancelText={t('editor.cancel')}
                onConfirm={() => analyzeOutline(outline.trim())}
            />
        </div>
    );
}
