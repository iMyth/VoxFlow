import { save } from '@tauri-apps/plugin-dialog';
import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';
import { AlertTriangle, CheckCircle, Loader2, Sparkles, ExternalLink, Folder, Wand2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { translateHyperframesStage } from './translateHyperframesStage';
import { extractErrorMessage } from '../../../lib/extractErrorMessage';
import * as ipc from '../../../lib/ipc';
import { useProjectStore } from '../../../store/projectStore';
import { Alert, AlertTitle, AlertDescription } from '../../ui/alert';
import { Button } from '../../ui/button';
import { Label } from '../../ui/label';
import { Progress } from '../../ui/progress';

interface HyperframesExportProps {
  lastExportedAudioPath: string | null;
}

export default function HyperframesExport({ lastExportedAudioPath }: HyperframesExportProps) {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);

  const [userPrompt, setUserPrompt] = useState('');

  // Export state
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState<{ percent: number; stage: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [outputDir, setOutputDir] = useState<string | null>(null);

  // Render state
  const [rendering, setRendering] = useState(false);
  const [renderProgress, setRenderProgress] = useState<{ percent: number; stage: string } | null>(null);
  const [renderDone, setRenderDone] = useState(false);
  const [renderError, setRenderError] = useState<string | null>(null);
  const [finalVideoPath, setFinalVideoPath] = useState<string | null>(null);

  const handleExport = async () => {
    if (!currentProject) return;

    const selectedPath = await save({
      title: t('export.exportVideoTitle'),
      defaultPath: `${currentProject.project.name ?? 'output'}.mp4`,
      filters: [{ name: 'MP4 Video', extensions: ['mp4'] }],
    });
    if (!selectedPath) return;

    const dir = selectedPath.replace(/\.mp4$/i, '_hyperframes');

    setExporting(true);
    setProgress(null);
    setError(null);
    setRendering(false);
    setRenderProgress(null);
    setRenderDone(false);
    setRenderError(null);
    setFinalVideoPath(null);

    const unlisten = await ipc.onHyperframesProgress((p: { percent: number; stage: string }) => {
      setProgress(p);
    });

    try {
      await ipc.exportHyperframes({
        project_id: currentProject.project.id,
        output_dir: dir,
        include_audio: !!lastExportedAudioPath,
        audio_path: lastExportedAudioPath,
        user_prompt: userPrompt.trim() || null,
      });
      setOutputDir(dir);
      unlisten();

      // Auto-render to video
      setRendering(true);
      const renderUnlisten = await ipc.onHyperframesRenderProgress((p: { percent: number; stage: string }) => {
        setRenderProgress(p);
      });

      try {
        const audioForRender = lastExportedAudioPath ? `${dir}/assets/audio.mp3` : null;
        const result = await ipc.renderHyperframesVideo({
          composition_dir: dir,
          output_path: selectedPath,
          audio_path: audioForRender,
        });
        setRenderDone(true);
        setFinalVideoPath(result);
      } catch (e) {
        setRenderError(extractErrorMessage(e));
      } finally {
        renderUnlisten();
        setRendering(false);
      }
    } catch (e) {
      setError(extractErrorMessage(e));
    } finally {
      unlisten();
      setExporting(false);
    }
  };

  return (
    <>
      {/* User prompt */}
      <div className="space-y-2">
        <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          <Wand2 className="h-3.5 w-3.5 inline mr-1" />
          {t('export.hyperframesUserPromptLabel')}
        </Label>
        <textarea
          className="w-full min-h-20 rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y"
          placeholder={t('export.hyperframesUserPromptPlaceholder')}
          value={userPrompt}
          onChange={(e) => {
            setUserPrompt(e.target.value);
          }}
        />
        <p className="text-xs text-muted-foreground">{t('export.hyperframesAiHint')}</p>
      </div>

      {/* Progress */}
      {exporting && progress && (
        <div className="rounded-lg bg-muted/50 p-3 space-y-2">
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            <span>{translateHyperframesStage(progress.stage, t)}</span>
            <span className="ml-auto font-mono">{Math.round(progress.percent)}%</span>
          </div>
          <Progress value={progress.percent} className="h-1.5" />
          <p className="text-[11px] text-muted-foreground/70 italic">{t('export.hyperframesPatience')}</p>
        </div>
      )}

      {rendering && renderProgress && (
        <div className="rounded-lg bg-muted/50 p-3 space-y-2">
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            <span>{renderProgress.stage}</span>
            <span className="ml-auto font-mono">{Math.round(renderProgress.percent)}%</span>
          </div>
          <Progress value={renderProgress.percent} className="h-1.5" />
        </div>
      )}

      {/* Errors */}
      {error && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('export.hyperframesExportFailed')}</AlertTitle>
          <AlertDescription className="text-xs">{error}</AlertDescription>
        </Alert>
      )}

      {renderError && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('export.renderFailed')}</AlertTitle>
          <AlertDescription className="text-xs space-y-1">
            <p>{renderError}</p>
            {outputDir && (
              <p className="text-muted-foreground">{t('export.renderHtmlGenerated', { path: outputDir })}</p>
            )}
          </AlertDescription>
        </Alert>
      )}

      {/* Success */}
      {renderDone && finalVideoPath && (
        <Alert>
          <CheckCircle className="h-4 w-4 text-green-500" />
          <AlertTitle>{t('export.videoExportDone')}</AlertTitle>
          <AlertDescription className="text-xs mt-1 space-y-2">
            <p className="text-muted-foreground truncate">{finalVideoPath}</p>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                className="gap-1.5 h-7 text-xs"
                onClick={() => void openPath(finalVideoPath)}
              >
                <ExternalLink className="h-3 w-3" />
                {t('export.openVideo')}
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="gap-1.5 h-7 text-xs"
                onClick={() => void revealItemInDir(finalVideoPath)}
              >
                <Folder className="h-3 w-3" />
                {t('export.openFolder')}
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      )}

      {/* Export button */}
      <Button
        className="w-full gap-2"
        variant="secondary"
        onClick={() => void handleExport()}
        disabled={exporting || rendering || !currentProject}
      >
        {exporting || rendering ? <Loader2 className="h-4 w-4 animate-spin" /> : <Sparkles className="h-4 w-4" />}
        {exporting
          ? t('export.hyperframesExporting')
          : rendering
            ? t('export.renderingVideo')
            : t('export.hyperframesExportButton')}
      </Button>
    </>
  );
}
