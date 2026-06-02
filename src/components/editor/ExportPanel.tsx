import { save } from '@tauri-apps/plugin-dialog';
import { Combine, Loader2, XCircle } from 'lucide-react';
import { useState, useEffect, useMemo, useCallback } from 'react';
import { useTranslation } from 'react-i18next';

import AudioExportStep from './export/AudioExportStep';
import ImportSection from './export/ImportSection';
import { extractErrorMessage } from '../../lib/extractErrorMessage';
import { mergeSectionVideos, onMergeProgress } from '../../lib/ipc';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import { useSectionVideoStore } from '../../store/sectionVideoStore';
import { Button } from '../ui/button';
import { Label } from '../ui/label';
import { Progress } from '../ui/progress';
import { Slider } from '../ui/slider';

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

  const { transitionDurationMs } = useSectionVideoStore();
  const { videoReady } = useSectionVideoStore();

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

  // Check if all sections have videos ready for merge
  const allSectionsReady = useMemo(
    () => sections.length > 0 && sections.every((s) => videoReady[s.id]),
    [sections, videoReady]
  );

  const readyCount = useMemo(() => sections.filter((s) => videoReady[s.id]).length, [sections, videoReady]);

  // Wire Tauri event listeners
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    void onMergeProgress((progress) => {
      setMergePercent(progress.percent);
      setMergeStage(progress.stage);
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

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
        <p className="text-sm text-muted-foreground mt-1">{t('export.mergeDesc')}</p>
      </div>

      <div className="space-y-6">
        {/* Audio Export (optional, independent section) */}
        <AudioExportStep
          audioReady={audioReady}
          missingLines={missingLines}
          onAudioExported={setLastExportedAudioPath}
        />

        {/* Video Merge */}
        <div className="rounded-xl border border-border bg-card overflow-hidden">
          {/* Section header */}
          <div className="flex items-center gap-3 px-5 py-3 border-b border-border/50 bg-muted/30">
            <div className="flex h-7 w-7 items-center justify-center rounded-full text-xs font-bold bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-400">
              2
            </div>
            <div className="flex-1">
              <h3 className="text-sm font-semibold">{t('export.mergeTitle')}</h3>
            </div>
          </div>

          <div className="px-5 py-4 space-y-4">
            {/* Status summary */}
            <div className="flex items-center gap-2 text-sm rounded-lg bg-muted/50 px-3 py-2">
              {allSectionsReady ? (
                <>
                  <span className="h-2 w-2 rounded-full bg-green-500" />
                  <span className="text-muted-foreground">{t('export.allSectionsReady', { count: readyCount })}</span>
                </>
              ) : (
                <>
                  <span className="h-2 w-2 rounded-full bg-gray-400" />
                  <span className="text-muted-foreground">
                    {t('export.sectionsReady', { ready: readyCount, total: sections.length })}
                  </span>
                </>
              )}
            </div>

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

            {/* Merge button */}
            <Button
              className="w-full gap-1.5"
              onClick={() => void handleMerge()}
              disabled={mergeInProgress || !allSectionsReady}
              size="lg"
            >
              {mergeInProgress ? <Loader2 className="h-4 w-4 animate-spin" /> : <Combine className="h-4 w-4" />}
              {mergeInProgress ? t('export.merging') : t('export.mergeAll')}
            </Button>

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
        </div>
      </div>

      {/* Import section */}
      <ImportSection />
    </div>
  );
}
