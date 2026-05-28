import { save } from '@tauri-apps/plugin-dialog';
import { AlertTriangle, Combine, Loader2, Play, XCircle } from 'lucide-react';
import { useState, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';

import AudioExportStep from './export/AudioExportStep';
import ImportSection from './export/ImportSection';
import SectionConfigCard from './export/SectionConfigCard';
import { extractErrorMessage } from '../../lib/extractErrorMessage';
import {
  generateSectionVideo,
  generateAllSections,
  mergeSectionVideos,
  cancelSectionGeneration,
  onSectionVideoProgress,
  onMergeProgress,
} from '../../lib/ipc';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import { useSectionVideoStore } from '../../store/sectionVideoStore';
import { Alert, AlertTitle, AlertDescription } from '../ui/alert';
import { Button } from '../ui/button';
import { Label } from '../ui/label';
import { Progress } from '../ui/progress';
import { Slider } from '../ui/slider';

import type { SectionStyleConfig } from '../../types';
import type { UnlistenFn } from '@tauri-apps/api/event';

export default function ExportPanel() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);
  const { lines } = useScriptStore();

  const [lastExportedAudioPath, setLastExportedAudioPath] = useState<string | null>(null);
  const [mergeInProgress, setMergeInProgress] = useState(false);
  const [mergePercent, setMergePercent] = useState(0);
  const [mergeStage, setMergeStage] = useState('');
  const [mergeError, setMergeError] = useState<string | null>(null);

  const {
    configs,
    statuses,
    batchInProgress,
    batchCompleted,
    batchTotal,
    transitionDurationMs,
    setConfig,
    setStatus,
    setBatchState,
  } = useSectionVideoStore();

  const sections = useMemo(
    () => [...(currentProject?.sections ?? [])].sort((a, b) => a.section_order - b.section_order),
    [currentProject?.sections]
  );

  const audioFragments = currentProject?.audio_fragments;
  const coveredLineIds = useMemo(() => new Set((audioFragments ?? []).map((a) => a.line_id)), [audioFragments]);
  const missingLines = useMemo(
    () => lines.filter((l) => l.text.trim() && !coveredLineIds.has(l.id)),
    [lines, coveredLineIds]
  );

  const audioReady = !!lastExportedAudioPath;

  // Progress summary counts
  const completedCount = useMemo(
    () => Object.values(statuses).filter((s) => s.state === 'completed').length,
    [statuses]
  );
  const failedCount = useMemo(() => Object.values(statuses).filter((s) => s.state === 'failed').length, [statuses]);
  const generatingCount = useMemo(
    () => Object.values(statuses).filter((s) => s.state === 'generating').length,
    [statuses]
  );
  const remainingCount = sections.length - completedCount - failedCount - generatingCount;

  // Check if at least one section is configured
  const hasConfiguredSections = useMemo(() => sections.some((s) => configs[s.id]?.mode), [sections, configs]);

  // All sections completed (for merge button)
  const allSectionsCompleted = useMemo(
    () => sections.length > 0 && sections.every((s) => statuses[s.id]?.state === 'completed'),
    [sections, statuses]
  );

  // Wire Tauri event listeners
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    // section-video-progress
    void onSectionVideoProgress((progress) => {
      setStatus(progress.section_id, {
        state: 'generating',
        percent: progress.percent,
        stage: progress.stage,
      });
    }).then((unlisten) => unlisteners.push(unlisten));

    // merge-progress
    void onMergeProgress((progress) => {
      setMergePercent(progress.percent);
      setMergeStage(progress.stage);
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [setStatus]);

  // Listen for section-video-complete and section-video-failed via the listen API
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    // We import listen directly for these custom events
    void import('@tauri-apps/api/event').then(({ listen }) => {
      void listen<{ section_id: string; video_path: string; duration_ms: number; file_size_bytes: number }>(
        'section-video-complete',
        (event) => {
          setStatus(event.payload.section_id, {
            state: 'completed',
            duration_ms: event.payload.duration_ms,
            file_size_bytes: event.payload.file_size_bytes,
          });
        }
      ).then((unlisten) => unlisteners.push(unlisten));

      void listen<{ section_id: string; error: string }>('section-video-failed', (event) => {
        setStatus(event.payload.section_id, {
          state: 'failed',
          error: event.payload.error,
        });
      }).then((unlisten) => unlisteners.push(unlisten));
    });

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [setStatus]);

  const handleGenerate = useCallback(
    async (sectionId: string) => {
      if (!currentProject) return;
      const config = configs[sectionId];
      if (!config) return;

      // Cancel if currently generating
      const currentStatus = statuses[sectionId];
      if (currentStatus?.state === 'generating') {
        await cancelSectionGeneration(sectionId);
      }

      setStatus(sectionId, { state: 'generating', percent: 0, stage: 'starting' });

      try {
        const result = await generateSectionVideo(currentProject.project.id, sectionId, config);
        setStatus(sectionId, {
          state: 'completed',
          duration_ms: result.duration_ms,
          file_size_bytes: result.file_size_bytes,
        });
      } catch (e) {
        setStatus(sectionId, {
          state: 'failed',
          error: extractErrorMessage(e).slice(0, 200),
        });
      }
    },
    [currentProject, configs, statuses, setStatus]
  );

  const handlePreview = useCallback(
    async (sectionId: string) => {
      // Preview opens the video file - handled by the system default player
      // The video path is stored in the completed status
      const status = statuses[sectionId];
      if (status?.state === 'completed') {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          await invoke('open_file', { path: (status as { state: 'completed'; video_path?: string }).video_path });
        } catch {
          // System default player failed to open - silently ignore
        }
      }
    },
    [statuses]
  );

  const handleGenerateAll = useCallback(async () => {
    if (!currentProject || batchInProgress) return;

    const sectionConfigs: [string, SectionStyleConfig][] = sections
      .filter((s) => configs[s.id]?.mode)
      .map((s) => [s.id, configs[s.id]]);

    if (sectionConfigs.length === 0) return;

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
    } catch (e) {
      setBatchState({ batchInProgress: false });
    }
  }, [currentProject, batchInProgress, sections, configs, setBatchState, setStatus]);

  const handleMerge = useCallback(async () => {
    if (!currentProject) return;

    const outputPath = await save({
      title: t('export.mergeSelectOutput'),
      filters: [{ name: 'Video', extensions: ['mp4'] }],
      defaultPath: `${currentProject.project.name}_final.mp4`,
    });

    if (!outputPath) return;

    setMergeInProgress(true);
    setMergePercent(0);
    setMergeStage('');
    setMergeError(null);

    try {
      await mergeSectionVideos(currentProject.project.id, outputPath, transitionDurationMs);
      setMergeInProgress(false);
    } catch (e) {
      setMergeInProgress(false);
      setMergeError(extractErrorMessage(e));
    }
  }, [currentProject, transitionDurationMs, t]);

  const handleTransitionChange = useCallback((value: number[]) => {
    useSectionVideoStore.setState({ transitionDurationMs: value[0] });
  }, []);

  return (
    <div className="mx-auto max-w-3xl px-6 py-8">
      {/* Page header */}
      <div className="mb-8">
        <h2 className="text-xl font-bold tracking-tight">{t('export.title')}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t('export.sectionVideoDesc')}</p>
      </div>

      {/* Missing audio warning */}
      {missingLines.length > 0 && (
        <Alert variant="destructive" className="mb-6">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('export.missingAudio', { count: missingLines.length })}</AlertTitle>
          <AlertDescription>
            <ul className="mt-1 space-y-0.5 text-xs">
              {missingLines.slice(0, 5).map((l) => (
                <li key={l.id}>{t('export.missingLine', { line: l.line_order + 1, text: l.text.slice(0, 40) })}</li>
              ))}
              {missingLines.length > 5 && <li>{t('export.missingMore', { count: missingLines.length - 5 })}</li>}
            </ul>
            <p className="mt-2 text-xs">{t('export.missingHint')}</p>
          </AlertDescription>
        </Alert>
      )}

      <div className="space-y-6">
        {/* Audio Export (optional, independent section) */}
        <AudioExportStep
          audioReady={audioReady}
          missingLines={missingLines}
          onAudioExported={setLastExportedAudioPath}
        />

        {/* Section Video Generation */}
        <div className="rounded-xl border border-border bg-card overflow-hidden">
          {/* Section header */}
          <div className="flex items-center gap-3 px-5 py-3 border-b border-border/50 bg-muted/30">
            <div className="flex h-7 w-7 items-center justify-center rounded-full text-xs font-bold bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-400">
              2
            </div>
            <div className="flex-1">
              <h3 className="text-sm font-semibold">{t('export.sectionVideoTitle')}</h3>
            </div>
          </div>

          <div className="px-5 py-4 space-y-4">
            {/* Overall progress summary */}
            {(generatingCount > 0 || completedCount > 0 || failedCount > 0) && (
              <div className="flex items-center gap-4 text-xs text-muted-foreground rounded-lg bg-muted/50 px-3 py-2">
                <span className="flex items-center gap-1.5">
                  <span className="h-2 w-2 rounded-full bg-green-500" />
                  {t('export.summaryCompleted', { count: completedCount })}
                </span>
                {failedCount > 0 && (
                  <span className="flex items-center gap-1.5">
                    <span className="h-2 w-2 rounded-full bg-red-500" />
                    {t('export.summaryFailed', { count: failedCount })}
                  </span>
                )}
                {generatingCount > 0 && (
                  <span className="flex items-center gap-1.5">
                    <span className="h-2 w-2 rounded-full bg-blue-500" />
                    {t('export.summaryGenerating', { count: generatingCount })}
                  </span>
                )}
                <span className="flex items-center gap-1.5">
                  <span className="h-2 w-2 rounded-full bg-gray-400" />
                  {t('export.summaryRemaining', { count: remainingCount })}
                </span>
              </div>
            )}

            {/* Batch progress */}
            {batchInProgress && (
              <div className="rounded-lg bg-muted/50 p-3 space-y-2">
                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  <span>{t('export.batchProgress', { completed: batchCompleted, total: batchTotal })}</span>
                </div>
                <Progress value={batchTotal > 0 ? (batchCompleted / batchTotal) * 100 : 0} className="h-1.5" />
              </div>
            )}

            {/* Section list */}
            {sections.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">{t('export.noSections')}</p>
            ) : (
              <div className="space-y-3">
                {sections.map((section) => (
                  <SectionConfigCard
                    key={section.id}
                    section={section}
                    config={configs[section.id] ?? { mode: 'template', template: 'minimal-subtitle' }}
                    status={statuses[section.id] ?? { state: 'not_started' }}
                    onConfigChange={(config) => {
                      setConfig(section.id, config);
                    }}
                    onGenerate={() => void handleGenerate(section.id)}
                    onPreview={() => void handlePreview(section.id)}
                  />
                ))}
              </div>
            )}

            {/* Batch controls */}
            {sections.length > 0 && (
              <div className="space-y-4 pt-2 border-t border-border/50">
                {/* Transition duration slider */}
                <div className="space-y-2">
                  <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                    {t('export.transitionDuration', { ms: transitionDurationMs })}
                  </Label>
                  <Slider
                    value={[transitionDurationMs]}
                    min={100}
                    max={2000}
                    step={50}
                    onValueChange={handleTransitionChange}
                  />
                  <div className="flex justify-between text-xs text-muted-foreground">
                    <span>100ms</span>
                    <span>2000ms</span>
                  </div>
                </div>

                {/* Generate All and Merge buttons */}
                <div className="flex items-center gap-3">
                  <Button
                    className="flex-1 gap-1.5"
                    onClick={() => void handleGenerateAll()}
                    disabled={batchInProgress || !hasConfiguredSections}
                  >
                    {batchInProgress ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
                    {batchInProgress ? t('export.batchGenerating') : t('export.generateAll')}
                  </Button>

                  <Button
                    variant="secondary"
                    className="flex-1 gap-1.5"
                    onClick={() => void handleMerge()}
                    disabled={mergeInProgress || !allSectionsCompleted}
                  >
                    {mergeInProgress ? <Loader2 className="h-4 w-4 animate-spin" /> : <Combine className="h-4 w-4" />}
                    {mergeInProgress ? t('export.merging') : t('export.mergeAll')}
                  </Button>
                </div>

                {/* Merge progress */}
                {mergeInProgress && (
                  <div className="rounded-lg bg-muted/50 p-3 space-y-2">
                    <div className="flex items-center gap-2 text-xs text-muted-foreground">
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      <span>{mergeStage}</span>
                      <span className="ml-auto font-mono">{Math.round(mergePercent)}%</span>
                    </div>
                    <Progress value={mergePercent} className="h-1.5" />
                  </div>
                )}

                {/* Merge error */}
                {mergeError && (
                  <div className="flex items-start gap-2 rounded-lg bg-destructive/10 border border-destructive/20 px-3 py-2">
                    <XCircle className="h-3.5 w-3.5 text-destructive shrink-0 mt-0.5" />
                    <span className="text-xs text-destructive">{mergeError}</span>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Import section */}
      <ImportSection />
    </div>
  );
}
