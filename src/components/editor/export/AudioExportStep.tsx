import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { save, open } from '@tauri-apps/plugin-dialog';
import { Download, CheckCircle, Loader2, FolderOpen, Play, Pause, AlertTriangle } from 'lucide-react';
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { extractErrorMessage } from '../../../lib/extractErrorMessage';
import * as ipc from '../../../lib/ipc';
import { useProjectStore } from '../../../store/projectStore';
import { Alert, AlertTitle, AlertDescription } from '../../ui/alert';
import { Button } from '../../ui/button';
import { Input } from '../../ui/input';
import { Label } from '../../ui/label';
import { Progress } from '../../ui/progress';
import { Slider } from '../../ui/slider';

import type { MixProgress } from '../../../types';

interface AudioExportStepProps {
  audioReady: boolean;
  missingLines: { id: string; line_order: number; text: string }[];
  onAudioExported: (path: string) => void;
}

export default function AudioExportStep({ audioReady, missingLines, onAudioExported }: AudioExportStepProps) {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);

  const [bgmPath, setBgmPath] = useState<string | null>(null);
  const [bgmVolume, setBgmVolume] = useState(0.3);
  const [bgmPlaying, setBgmPlaying] = useState(false);
  const [outputPath, setOutputPath] = useState('');
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState<MixProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (currentProject) {
      setOutputPath(`${currentProject.project.name}.mp3`);
    }
  }, [currentProject]);

  useEffect(() => {
    const unlisten = listen('audio-finished', () => {
      setBgmPlaying(false);
    });
    return () => {
      void unlisten.then((fn) => {
        fn();
      });
      if (bgmPlaying) {
        invoke('stop_audio').catch(() => {});
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleBgmBrowse = async () => {
    const selected = await open({
      title: t('export.selectBgm'),
      multiple: false,
      filters: [{ name: 'Audio Files', extensions: ['mp3', 'wav', 'flac', 'ogg', 'm4a', 'aac'] }],
    });
    if (selected) {
      const path = Array.isArray(selected) ? (selected[0] as string) : selected;
      setBgmPath(path);
    }
  };

  const toggleBgmPreview = async () => {
    if (!bgmPath) return;
    try {
      if (bgmPlaying) {
        await invoke('stop_audio');
        setBgmPlaying(false);
      } else {
        await invoke('play_audio', { filePath: bgmPath });
        setBgmPlaying(true);
      }
    } catch {
      setBgmPlaying(false);
    }
  };

  const handleBgmVolumeChange = async (value: number[]) => {
    const vol = value[0];
    setBgmVolume(vol);
    if (bgmPlaying) {
      try {
        await invoke('set_audio_volume', { volume: vol });
      } catch {
        // ignore
      }
    }
  };

  const handleExport = async () => {
    if (!currentProject || missingLines.length > 0) return;

    const selectedPath = await save({
      title: t('editor.exportAudiobookTitle'),
      defaultPath: outputPath,
      filters: [{ name: 'MP3 Audio', extensions: ['mp3'] }],
    });
    if (!selectedPath) return;

    setExporting(true);
    setProgress(null);
    setError(null);

    const unlisten = await ipc.onMixProgress((p) => {
      setProgress(p);
    });

    try {
      await ipc.exportAudioMix(currentProject.project.id, selectedPath, bgmPath, bgmVolume);
      onAudioExported(selectedPath);
    } catch (e) {
      setError(extractErrorMessage(e));
    } finally {
      unlisten();
      setExporting(false);
    }
  };

  return (
    <div className="relative rounded-xl border border-border bg-card overflow-hidden">
      {/* Step header */}
      <div className="flex items-center gap-3 px-5 py-3 border-b border-border/50 bg-muted/30">
        <div
          className={`flex h-7 w-7 items-center justify-center rounded-full text-xs font-bold ${audioReady ? 'bg-green-100 text-green-700 dark:bg-green-900/40 dark:text-green-400' : 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-400'}`}
        >
          {audioReady ? <CheckCircle className="h-4 w-4" /> : '1'}
        </div>
        <div className="flex-1">
          <h3 className="text-sm font-semibold">{t('export.exportButton')}</h3>
        </div>
        {audioReady && (
          <span className="text-xs text-green-600 dark:text-green-400 font-medium">{t('export.exportSuccess')}</span>
        )}
      </div>

      {/* Step content */}
      <div className="px-5 py-4 space-y-4">
        {/* BGM section */}
        <div className="space-y-3">
          <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">{t('export.bgm')}</Label>
          <div className="flex gap-2 items-center">
            <Input
              className="flex-1 h-9"
              placeholder={t('export.bgmPlaceholder')}
              value={bgmPath ?? ''}
              onChange={(e) => {
                setBgmPath(e.target.value || null);
              }}
            />
            <Button
              variant="outline"
              size="icon"
              className="h-9 w-9 shrink-0"
              onClick={() => void handleBgmBrowse()}
              title={t('export.browse')}
            >
              <FolderOpen className="h-4 w-4" />
            </Button>
            {bgmPath && (
              <Button
                variant="outline"
                size="icon"
                className="h-9 w-9 shrink-0"
                onClick={() => void toggleBgmPreview()}
                title={bgmPlaying ? t('editor.pause') : t('editor.play')}
              >
                {bgmPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
              </Button>
            )}
          </div>
          {bgmPath && (
            <div className="space-y-1.5">
              <Label className="text-xs">{t('export.bgmVolume', { percent: Math.round(bgmVolume * 100) })}</Label>
              <Slider
                min={0}
                max={1}
                step={0.05}
                value={[bgmVolume]}
                onValueChange={(v) => void handleBgmVolumeChange(v)}
              />
            </div>
          )}
        </div>

        {/* Output filename */}
        <div className="space-y-1.5">
          <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
            {t('export.outputLabel')}
          </Label>
          <Input
            className="h-9"
            value={outputPath}
            onChange={(e) => {
              setOutputPath(e.target.value);
            }}
          />
        </div>

        {/* Progress */}
        {exporting && progress && (
          <div className="rounded-lg bg-muted/50 p-3 space-y-2">
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              <span>{progress.stage}</span>
              <span className="ml-auto font-mono">{Math.round(progress.percent)}%</span>
            </div>
            <Progress value={progress.percent} className="h-1.5" />
          </div>
        )}

        {error && (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>{t('export.exportFailed')}</AlertTitle>
            <AlertDescription className="text-xs">{error}</AlertDescription>
          </Alert>
        )}

        {/* Export button */}
        <Button
          className="w-full gap-2"
          onClick={() => void handleExport()}
          disabled={exporting || missingLines.length > 0 || !outputPath.trim()}
        >
          {exporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
          {exporting ? t('export.exporting') : t('export.exportButton')}
        </Button>
      </div>
    </div>
  );
}
