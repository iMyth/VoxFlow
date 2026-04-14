import { useEffect, useRef, useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import OutlinePanel from './OutlinePanel';
import PlanCard from './PlanCard';
import ScriptLines from './ScriptLines';
import StreamingCard from './StreamingCard';
import ThinkingPanel from './ThinkingPanel';
import ToolCallList from './ToolCallList';
import { useCharacterStore } from '../../store/characterStore';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import { useSettingsStore } from '../../store/settingsStore';
import { useToastStore } from '../../store/toastStore';
import { useUiStore } from '../../store/uiStore';
import ConfirmDialog from '../ui/confirm-dialog';

export default function ScriptEditor() {
  const { t } = useTranslation();
  const {
    lines,
    sections,
    isGenerating,
    isAnalyzing,
    isDirty,
    streamingText,
    thinkingText,
    toolCalls,
    enableThinking,
    setEnableThinking,
    agentPlan,
    workflow,
    setWorkflow,
    analyzeOutline,
    setAgentPlan,
    generateScript,
    cancelLlm,
    saveScript,
    generateAllTts,
    regenerateAllTts,
    isBatchTtsRunning,
    batchTtsProgress,
  } = useScriptStore();
  const currentProject = useProjectStore((s) => s.currentProject);
  const existingCharacters = useCharacterStore((s) => s.characters);
  const [outline, setOutline] = useState('');
  const [showOverwriteConfirm, setShowOverwriteConfirm] = useState(false);
  const [showAnalyzeConfirm, setShowAnalyzeConfirm] = useState(false);
  const [showRegenerateConfirm, setShowRegenerateConfirm] = useState(false);
  const [showOutlineDialog, setShowOutlineDialog] = useState(false);
  const [confirmData, setConfirmData] = useState({ textCount: 0, audioCount: 0 });
  const [extraInstructions, setExtraInstructions] = useState('');
  const [characterMapping, setCharacterMapping] = useState<Record<string, string>>({});
  const [creatingChars, setCreatingChars] = useState<Record<string, boolean>>({});
  const [newCharForms, setNewCharForms] = useState<
    Record<string, { name: string; voice: string; speed: number; pitch: number }>
  >({});
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
        // New project with no existing lines: default to AI mode
        if (useScriptStore.getState().lines.length > 0) {
          useScriptStore.getState().setWorkflow('manual');
        } else {
          useScriptStore.getState().setWorkflow('ai');
        }
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    outlineSaveTimerRef.current = setTimeout(() => {
      void (async () => {
        await useProjectStore.getState().saveOutline(outlineRef.current);
      })();
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
        void saveScript();
      }, 5000);
    }

    return () => {
      if (saveTimerRef.current) {
        clearTimeout(saveTimerRef.current);
      }
    };
  }, [isDirty, lines, saveScript]);

  // ---- Count missing TTS lines ----
  const audioFragments = currentProject?.audio_fragments;
  const coveredLineIds = useMemo(() => new Set((audioFragments ?? []).map((a) => a.line_id)), [audioFragments]);
  const missingTtsCount = useMemo(
    () => lines.filter((l) => l.text.trim() && !coveredLineIds.has(l.id)).length,
    [lines, coveredLineIds]
  );

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

    void analyzeOutline(outline.trim());
  };

  // ---- Plan actions ----
  const handleCharacterMapping = (suggestedName: string, targetId: string) => {
    setCharacterMapping((prev) => ({ ...prev, [suggestedName]: targetId }));
    if (targetId === '__new__') {
      setCreatingChars((prev) => ({ ...prev, [suggestedName]: true }));
      setNewCharForms((prev) => {
        const defaults = useSettingsStore.getState();
        return {
          ...prev,
          [suggestedName]: {
            name: suggestedName,
            voice: defaults.defaultVoiceName,
            speed: defaults.defaultSpeed,
            pitch: defaults.defaultPitch,
          },
        };
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
    const unmapped = agentPlan.suggested_characters.filter((c) => !characterMapping[c.name]);
    if (unmapped.length > 0) {
      useToastStore.getState().addToast('editor.mapAllCharsRequired');
      return;
    }
    const pendingCreate = agentPlan.suggested_characters.filter((c) => creatingChars[c.name]);
    if (pendingCreate.length > 0) {
      const names = pendingCreate.map((c) => c.name).join(', ');
      useToastStore.getState().addToast(t('editor.finishCreatingChars', { names }));
      return;
    }

    const mappingText = Object.entries(characterMapping)
      .map(([suggested, target]) => `  "${suggested}" -> "${target}"`)
      .join('\n');
    const fullInstructions = [extraInstructions.trim(), `\n角色映射：\n${mappingText}`].filter(Boolean).join('\n');

    const confirmedCharNames = Object.values(characterMapping);

    const existingText = lines.filter((l) => l.text.trim()).length;
    const existingAudio = coveredLineIds.size;
    if (existingText > 0 || existingAudio > 0) {
      setConfirmData({ textCount: existingText, audioCount: existingAudio });
      setShowOverwriteConfirm(true);
      useUiStore.getState().setPendingExtraInstructions(fullInstructions);
      useUiStore.getState().setPendingCharNames(confirmedCharNames);
      return;
    }

    void generateScript(outline.trim(), fullInstructions, confirmedCharNames);
    setAgentPlan(null);
  };

  const handlePlanManualMode = () => {
    setAgentPlan(null);
    setWorkflow('manual');
  };

  const isAiMode = workflow === 'ai';

  return (
    <div className="px-6 py-4 space-y-4 relative">
      {/* Batch TTS blocking overlay */}
      {isBatchTtsRunning && (
        <div
          className="fixed z-50 flex items-center justify-center bg-background/60 backdrop-blur-sm pointer-events-auto"
          style={{ top: 0, right: 0, bottom: 0, left: 0, width: '100vw', height: '100vh' }}
        >
          <div className="card p-6 space-y-3 text-center max-w-sm">
            <p className="text-sm font-medium">
              {t('editor.batchTtsRunning', {
                current: batchTtsProgress?.current ?? 0,
                total: batchTtsProgress?.total ?? 0,
              })}
            </p>
            {batchTtsProgress && batchTtsProgress.total > 0 && (
              <div className="space-y-1">
                <div className="w-full bg-secondary rounded-full h-2 overflow-hidden">
                  <div
                    className="h-full bg-primary rounded-full transition-all duration-300"
                    style={{ width: `${String((batchTtsProgress.current / batchTtsProgress.total) * 100)}%` }}
                  />
                </div>
                <p className="text-xs text-muted-foreground">
                  {batchTtsProgress.current} / {batchTtsProgress.total}
                </p>
              </div>
            )}
            <p className="text-xs text-muted-foreground">{t('editor.batchTtsHint')}</p>
          </div>
        </div>
      )}

      {/* Outline dialog */}
      <OutlinePanel
        outline={outline}
        onOutlineChange={setOutline}
        isAnalyzing={isAnalyzing}
        enableThinking={enableThinking}
        onToggleThinking={setEnableThinking}
        onAnalyze={handleAnalyze}
        onCancel={() => void cancelLlm()}
        hasAgentPlan={!!agentPlan}
        open={showOutlineDialog}
        onOpenChange={setShowOutlineDialog}
      />

      {/* Analyzing streaming */}
      {isAnalyzing && (
        <StreamingCard
          color="blue"
          label={t('editor.analyzing')}
          text={streamingText}
          onCancel={() => void cancelLlm()}
        />
      )}

      {/* Generating streaming */}
      {isGenerating && streamingText && (
        <StreamingCard
          color="purple"
          label={t('editor.aiGenerating')}
          text={streamingText}
          onCancel={() => void cancelLlm()}
        />
      )}

      {/* Thinking panel - shown during analysis or generation when thinking content is present */}
      {(isAnalyzing || isGenerating) && thinkingText && (
        <ThinkingPanel thinkingText={thinkingText} isThinking={isAnalyzing || isGenerating} />
      )}

      {/* Tool calls - shown during generation when tools are invoked */}
      {toolCalls.length > 0 && <ToolCallList entries={toolCalls} />}

      {/* Plan card - AI mode only */}
      {isAiMode && agentPlan && (
        <PlanCard
          plan={agentPlan}
          existingCharacters={existingCharacters}
          currentProjectId={currentProject?.project.id ?? ''}
          onDismiss={() => {
            setAgentPlan(null);
          }}
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
        showOutlineBtn={workflow !== 'manual'}
        onEditOutline={() => {
          setShowOutlineDialog(true);
        }}
        workflow={workflow}
        onSelectAi={handleAiModeSelect}
        onSelectManual={handleManualModeSelect}
        isDirty={isDirty}
        isBatchTtsRunning={isBatchTtsRunning}
        batchTtsProgress={batchTtsProgress}
        missingTtsCount={missingTtsCount}
        hasAudioCount={coveredLineIds.size}
        onSave={() => void saveScript()}
        onGenerateAllTts={() => void generateAllTts()}
        onRegenerateAllTts={() => {
          setShowRegenerateConfirm(true);
        }}
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
          const { pendingExtraInstructions, pendingCharNames, clearPending } = useUiStore.getState();
          if (pendingExtraInstructions) {
            void generateScript(outline.trim(), pendingExtraInstructions, pendingCharNames ?? undefined);
            setAgentPlan(null);
            clearPending();
          } else {
            void generateScript(outline.trim());
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
        onConfirm={() => void analyzeOutline(outline.trim())}
      />

      <ConfirmDialog
        open={showRegenerateConfirm}
        onOpenChange={setShowRegenerateConfirm}
        title={t('editor.confirmRegenerateTitle')}
        description={t('editor.confirmRegenerate', { count: coveredLineIds.size })}
        confirmText={t('editor.confirmRegenerateBtn')}
        cancelText={t('editor.cancel')}
        irreversibleWarning={t('editor.irreversibleWarning')}
        onConfirm={() => void regenerateAllTts()}
      />
    </div>
  );
}
