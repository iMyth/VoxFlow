import { Plus, Undo2, Redo2, Wand2, X, Volume2, RefreshCw, Save, Sparkles, Video } from 'lucide-react';
import { useState, useEffect, useRef, useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import { GlobalStyleConfig } from './GlobalStyleConfig';
import ModeSelector from './ModeSelector';
import ScriptLineComponent from './ScriptLine';
import SectionGroup from './SectionGroup';
import { useDragAndDrop } from '../../hooks/useDragAndDrop';
import { generateAllSections } from '../../lib/ipc';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import { useSectionVideoStore } from '../../store/sectionVideoStore';
import { useToastStore } from '../../store/toastStore';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Progress } from '../ui/progress';

import type { ScriptLine, ScriptSection, SectionStyleConfig } from '../../types';

interface ScriptLinesProps {
  lines: ScriptLine[];
  sections: ScriptSection[];
  emptyHint: string;
  showOutlineBtn?: boolean;
  onEditOutline?: () => void;
  workflow: 'ai' | 'manual' | null;
  onSelectAi: () => void;
  onSelectManual: () => void;
  isDirty?: boolean;
  isBatchTtsRunning?: boolean;
  batchTtsProgress?: { current: number; total: number } | null;
  missingTtsCount?: number;
  hasAudioCount?: number;
  onSave?: () => void;
  onGenerateAllTts?: () => void;
  onRegenerateAllTts?: () => void;
  projectId: string;
}

export default function ScriptLines({
  lines,
  sections,
  emptyHint,
  showOutlineBtn,
  onEditOutline,
  workflow,
  onSelectAi,
  onSelectManual,
  isDirty,
  isBatchTtsRunning,
  batchTtsProgress,
  missingTtsCount,
  hasAudioCount,
  onSave,
  onGenerateAllTts,
  onRegenerateAllTts,
  projectId,
}: ScriptLinesProps) {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);
  const globalVideoStyle = useScriptStore((s) => s.globalVideoStyle);
  const { addLine, addSection, setAllInstructions, reorderLines } = useScriptStore();
  const { configs, audioReady, setStatus, setBatchState, setVideoReady, batchInProgress } = useSectionVideoStore();
  const addToast = useToastStore((s) => s.addToast);
  const [batchInstructionsOpen, setBatchInstructionsOpen] = useState(false);
  const [batchInstructionsValue, setBatchInstructionsValue] = useState('');
  const [outlineBtnBouncing, setOutlineBtnBouncing] = useState(false);
  const outlineBtnAnimated = useRef(false);

  const { draggingId, dropTarget, handleDragStart, handleDragMove, handleDragEnd } = useDragAndDrop({
    getLines: () => useScriptStore.getState().lines,
    reorderFn: (fromIdx, toIdx) => {
      reorderLines(fromIdx, toIdx);
    },
  });

  useEffect(() => {
    if (showOutlineBtn && !outlineBtnAnimated.current) {
      outlineBtnAnimated.current = true;
      setOutlineBtnBouncing(true);
      const timer = setTimeout(() => {
        setOutlineBtnBouncing(false);
      }, 1000);
      return () => {
        clearTimeout(timer);
      };
    }
  }, [showOutlineBtn]);

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

  const handleGenerateAllVideos = async () => {
    if (!currentProject || batchInProgress) return;

    // Find sections with audio ready
    const sectionsToGenerate = sortedSections.filter((section) => audioReady[section.id]);

    if (sectionsToGenerate.length === 0) {
      addToast(t('editor.batchVideoNoReady'), 'error');
      return;
    }

    // Prepare configs for each section
    const sectionConfigs: [string, SectionStyleConfig][] = sectionsToGenerate.map((section) => {
      const existingConfig = configs[section.id];
      const effectivePrompt =
        existingConfig?.useGlobalStyle === false
          ? (existingConfig.customStyle ?? '')
          : (existingConfig?.user_prompt ?? globalVideoStyle ?? '');

      return [
        section.id,
        {
          mode: 'agent',
          user_prompt: effectivePrompt,
          useGlobalStyle: existingConfig?.useGlobalStyle ?? true,
          customStyle: existingConfig?.customStyle,
        },
      ];
    });

    setBatchState({
      batchInProgress: true,
      batchCompleted: 0,
      batchFailed: 0,
      batchTotal: sectionConfigs.length,
    });

    // Set all sections to generating
    for (const [sectionId] of sectionConfigs) {
      setStatus(sectionId, { state: 'generating', percent: 0, stage: 'queued' });
    }

    try {
      const result = await generateAllSections(currentProject.project.id, sectionConfigs);

      setBatchState({
        batchInProgress: false,
        batchCompleted: result.completed.length,
        batchFailed: result.failed.length,
      });

      // Update videoReady for completed sections
      for (const sectionId of result.completed) {
        setVideoReady(sectionId, true);
      }

      if (result.failed.length > 0) {
        addToast(t('editor.batchVideoPartial', { failed: result.failed.length }), 'error');
      } else {
        addToast(t('editor.batchVideoSuccess', { count: result.completed.length }), 'success');
      }
    } catch (error) {
      setBatchState({ batchInProgress: false });
      addToast(t('editor.batchVideoFailed'), 'error');
      console.error('Batch video generation failed:', error);
    }
  };

  const hasLines = lines.length > 0;

  const sortedSections = useMemo(() => [...sections].sort((a, b) => a.section_order - b.section_order), [sections]);

  const linesBySection = useMemo(() => {
    const map = new Map<string, ScriptLine[]>();
    for (const line of lines) {
      if (line.section_id) {
        const entry = map.get(line.section_id);
        if (entry) {
          entry.push(line);
        } else {
          map.set(line.section_id, [line]);
        }
      }
    }
    return map;
  }, [lines]);

  const unassignedLines = useMemo(() => lines.filter((l) => !l.section_id), [lines]);

  const Toolbar = (
    <div className="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border/50 -mx-1 px-1 pb-3 mb-4">
      {/* Primary row: mode selector + actions */}
      <div className="flex items-center justify-between gap-3 pt-2">
        <div className="flex items-center gap-2">
          <ModeSelector onSelectAi={onSelectAi} onSelectManual={onSelectManual} currentMode={workflow ?? null} />

          {showOutlineBtn && onEditOutline && (
            <Button
              variant="outline"
              size="sm"
              className="h-7 px-3 text-xs gap-1.5 border-blue-200 dark:border-blue-800 text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20"
              onClick={onEditOutline}
              style={outlineBtnBouncing ? { animation: 'bounce-once 0.8s ease 1' } : undefined}
            >
              <Sparkles className="h-3.5 w-3.5" />
              {t('editor.editOutline')}
            </Button>
          )}
        </div>

        <div className="flex items-center gap-1.5">
          {isDirty && (
            <Button variant="ghost" size="sm" className="h-7 px-2.5 text-xs gap-1.5" onClick={onSave}>
              <Save className="h-3.5 w-3.5" />
              {t('editor.save')}
            </Button>
          )}

          {hasLines && (
            <>
              <div className="h-4 w-px bg-border mx-1" />
              <Button
                variant="ghost"
                size="sm"
                className="h-7 w-7 p-0"
                onClick={() => {
                  useScriptStore.temporal.getState().undo();
                }}
                title={`${t('editor.undo')} (⌘Z)`}
              >
                <Undo2 className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="h-7 w-7 p-0"
                onClick={() => {
                  useScriptStore.temporal.getState().redo();
                }}
                title={`${t('editor.redo')} (⇧⌘Z)`}
              >
                <Redo2 className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className={`h-7 w-7 p-0 ${batchInstructionsOpen ? 'text-purple-500 bg-purple-50 dark:bg-purple-950/30' : ''}`}
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
            </>
          )}
        </div>
      </div>

      {/* Batch instructions input */}
      {batchInstructionsOpen && (
        <div className="flex items-center gap-2 mt-2.5 animate-in fade-in slide-in-from-top-1 duration-150">
          <Input
            value={batchInstructionsValue}
            onChange={(e) => {
              setBatchInstructionsValue(e.target.value);
            }}
            onKeyDown={handleBatchKeyDown}
            className="h-8 text-xs flex-1 border-purple-200 dark:border-purple-800 focus-visible:border-purple-500"
            placeholder={t('editor.instructionsPlaceholder')}
            autoFocus
          />
          <Button
            size="sm"
            className="h-8 text-xs px-3"
            onClick={handleBatchInstructions}
            disabled={!batchInstructionsValue.trim()}
          >
            {t('editor.setAllInstructions')}
          </Button>
        </div>
      )}

      {/* TTS batch actions */}
      {((missingTtsCount ?? 0) > 0 || ((hasAudioCount ?? 0) > 0 && (missingTtsCount ?? 0) === 0)) && (
        <div className="flex items-center gap-2 mt-2.5">
          {(missingTtsCount ?? 0) > 0 && onGenerateAllTts && (
            <Button
              variant="outline"
              size="sm"
              className="h-7 px-3 text-xs gap-1.5 border-blue-200 dark:border-blue-800 text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20"
              onClick={onGenerateAllTts}
              disabled={isBatchTtsRunning}
            >
              <Volume2 className="h-3.5 w-3.5" />
              {isBatchTtsRunning
                ? t('editor.batchTtsRunning', {
                    current: batchTtsProgress?.current ?? 0,
                    total: batchTtsProgress?.total ?? missingTtsCount,
                  })
                : t('editor.generateAllTts', { count: missingTtsCount })}
            </Button>
          )}
          {(missingTtsCount ?? 0) === 0 && (hasAudioCount ?? 0) > 0 && onRegenerateAllTts && (
            <Button
              variant="outline"
              size="sm"
              className="h-7 px-3 text-xs gap-1.5 border-orange-200 dark:border-orange-800 text-orange-600 dark:text-orange-400 hover:bg-orange-50 dark:hover:bg-orange-900/20"
              onClick={onRegenerateAllTts}
              disabled={isBatchTtsRunning}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              {isBatchTtsRunning
                ? t('editor.batchTtsRunning', {
                    current: batchTtsProgress?.current ?? 0,
                    total: batchTtsProgress?.total ?? hasAudioCount,
                  })
                : t('editor.regenerateAllTts', { count: hasAudioCount })}
            </Button>
          )}
          {isBatchTtsRunning && batchTtsProgress && (
            <div className="flex-1 min-w-[120px] space-y-1">
              <Progress value={(batchTtsProgress.current / batchTtsProgress.total) * 100} className="h-1.5" />
            </div>
          )}
        </div>
      )}

      {/* Video batch actions */}
      {sortedSections.length > 0 && (
        <div className="flex items-center gap-2 mt-2.5">
          <Button
            variant="outline"
            size="sm"
            className="h-7 px-3 text-xs gap-1.5 border-purple-200 dark:border-purple-800 text-purple-600 dark:text-purple-400 hover:bg-purple-50 dark:hover:bg-purple-900/20"
            onClick={() => void handleGenerateAllVideos()}
            disabled={batchInProgress}
          >
            <Video className="h-3.5 w-3.5" />
            {batchInProgress
              ? t('editor.batchVideoGenerating', {
                  current: useSectionVideoStore.getState().batchCompleted + useSectionVideoStore.getState().batchFailed,
                  total: useSectionVideoStore.getState().batchTotal,
                })
              : t('editor.generateAllVideos', { count: sortedSections.filter((s) => audioReady[s.id]).length })}
          </Button>
          {batchInProgress && (
            <div className="flex-1 min-w-[120px] space-y-1">
              <Progress
                value={
                  useSectionVideoStore.getState().batchTotal > 0
                    ? ((useSectionVideoStore.getState().batchCompleted + useSectionVideoStore.getState().batchFailed) /
                        useSectionVideoStore.getState().batchTotal) *
                      100
                    : 0
                }
                className="h-1.5"
              />
            </div>
          )}
        </div>
      )}
    </div>
  );

  // Section-based layout
  if (sortedSections.length > 0) {
    return (
      <div className="relative">
        {Toolbar}
        <div className="space-y-6">
          <GlobalStyleConfig />
          {sortedSections.map((section, index) => (
            <SectionGroup
              key={section.id}
              section={section}
              lines={linesBySection.get(section.id) ?? []}
              index={index}
              totalSections={sortedSections.length}
              onAddLine={() => {
                addLine(-1, section.id);
              }}
              projectId={projectId}
            />
          ))}
          {unassignedLines.length > 0 && (
            <div className="space-y-2">
              {unassignedLines.map((line, index) => (
                <ScriptLineComponent
                  key={line.id}
                  line={line}
                  index={index}
                  isDragging={draggingId === line.id}
                  dropPosition={dropTarget?.id === line.id ? dropTarget.position : null}
                  onDragStart={handleDragStart}
                  onDragMove={handleDragMove}
                  onDragEnd={handleDragEnd}
                />
              ))}
            </div>
          )}
          <Button
            variant="outline"
            className="w-full border-dashed text-muted-foreground hover:text-foreground hover:border-solid transition-all"
            onClick={addSection}
          >
            <Plus className="h-4 w-4" /> {t('editor.addSection')}
          </Button>
        </div>
      </div>
    );
  }

  // Flat list layout
  return (
    <div className="relative">
      {Toolbar}
      <div className="space-y-2">
        {lines.length === 0 && (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-full bg-muted">
              <Plus className="h-6 w-6 text-muted-foreground/60" />
            </div>
            <p className="text-sm text-muted-foreground max-w-xs">{emptyHint}</p>
            {workflow === 'manual' && (
              <Button
                variant="outline"
                className="mt-4 border-dashed text-muted-foreground hover:text-foreground hover:border-solid transition-all"
                onClick={() => {
                  addLine(-1);
                }}
              >
                <Plus className="h-4 w-4" /> {t('editor.addLine')}
              </Button>
            )}
          </div>
        )}
        {lines.map((line, index) => (
          <ScriptLineComponent
            key={line.id}
            line={line}
            index={index}
            isDragging={draggingId === line.id}
            dropPosition={dropTarget?.id === line.id ? dropTarget.position : null}
            onDragStart={handleDragStart}
            onDragMove={handleDragMove}
            onDragEnd={handleDragEnd}
          />
        ))}
        {lines.length > 0 && (
          <Button
            variant="outline"
            className="w-full border-dashed text-muted-foreground hover:text-foreground hover:border-solid transition-all"
            onClick={() => {
              addLine(lines.length - 1);
            }}
          >
            <Plus className="h-4 w-4" /> {t('editor.addLine')}
          </Button>
        )}
      </div>
    </div>
  );
}
