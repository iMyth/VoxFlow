import { AlertTriangle, ArrowRight } from 'lucide-react';
import { useState, useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import AudioExportStep from './export/AudioExportStep';
import ImportSection from './export/ImportSection';
import VideoExportStep from './export/VideoExportStep';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import { Alert, AlertTitle, AlertDescription } from '../ui/alert';

export default function ExportPanel() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);
  const { lines } = useScriptStore();

  const [lastExportedAudioPath, setLastExportedAudioPath] = useState<string | null>(null);

  const audioFragments = currentProject?.audio_fragments;
  const coveredLineIds = useMemo(() => new Set((audioFragments ?? []).map((a) => a.line_id)), [audioFragments]);
  const missingLines = useMemo(
    () => lines.filter((l) => l.text.trim() && !coveredLineIds.has(l.id)),
    [lines, coveredLineIds]
  );

  const audioReady = !!lastExportedAudioPath;

  return (
    <div className="mx-auto max-w-3xl px-6 py-8">
      {/* Page header */}
      <div className="mb-8">
        <h2 className="text-xl font-bold tracking-tight">{t('export.title')}</h2>
        <p className="text-sm text-muted-foreground mt-1">{t('export.videoNeedAudioFirst')}</p>
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

      {/* Pipeline Steps */}
      <div className="space-y-4">
        {/* Step 1: Audio Export */}
        <AudioExportStep
          audioReady={audioReady}
          missingLines={missingLines}
          onAudioExported={setLastExportedAudioPath}
        />

        {/* Arrow connector */}
        <div className="flex justify-center">
          <div
            className={`flex items-center gap-1.5 text-xs ${audioReady ? 'text-green-600 dark:text-green-400' : 'text-muted-foreground/40'}`}
          >
            <ArrowRight className="h-4 w-4" />
          </div>
        </div>

        {/* Step 2: Video Export */}
        <VideoExportStep audioReady={audioReady} lastExportedAudioPath={lastExportedAudioPath} />
      </div>

      {/* Import section */}
      <ImportSection />
    </div>
  );
}
