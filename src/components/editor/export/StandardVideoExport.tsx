import { save, open } from '@tauri-apps/plugin-dialog';
import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';
import { AlertTriangle, CheckCircle, Loader2, Video, Image, ExternalLink, Folder } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { extractErrorMessage } from '../../../lib/extractErrorMessage';
import * as ipc from '../../../lib/ipc';
import { useProjectStore } from '../../../store/projectStore';
import { Alert, AlertTitle, AlertDescription } from '../../ui/alert';
import { Button } from '../../ui/button';
import { Input } from '../../ui/input';
import { Label } from '../../ui/label';
import { Progress } from '../../ui/progress';

import type { VideoStyle } from '../../../lib/ipc';
import type { MixProgress } from '../../../types';

interface StandardVideoExportProps {
  audioReady: boolean;
  lastExportedAudioPath: string | null;
  videoStyle: VideoStyle;
  videoFgColor: string;
  videoBgColor: string;
  onFgColorChange: (color: string) => void;
  onBgColorChange: (color: string) => void;
}

export default function StandardVideoExport({
  audioReady,
  lastExportedAudioPath,
  videoStyle,
  videoFgColor,
  videoBgColor,
  onFgColorChange,
  onBgColorChange,
}: StandardVideoExportProps) {
  const { t } = useTranslation();
  const currentProject = useProjectStore((s) => s.currentProject);

  const [videoBgImage, setVideoBgImage] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [progress, setProgress] = useState<MixProgress | null>(null);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [outputPath, setOutputPath] = useState<string | null>(null);

  const handleBgImageBrowse = async () => {
    const selected = await open({
      title: t('export.selectBgImage'),
      multiple: false,
      filters: [{ name: 'Image Files', extensions: ['png', 'jpg', 'jpeg', 'webp', 'bmp'] }],
    });
    if (selected) {
      const path = Array.isArray(selected) ? (selected[0] as string) : selected;
      setVideoBgImage(path);
    }
  };

  const handleExport = async () => {
    if (!lastExportedAudioPath) return;

    const selectedPath = await save({
      title: t('export.exportVideoTitle'),
      defaultPath: `${currentProject?.project.name ?? 'output'}.mp4`,
      filters: [{ name: 'MP4 Video', extensions: ['mp4'] }],
    });
    if (!selectedPath) return;

    setExporting(true);
    setProgress(null);
    setDone(false);
    setError(null);

    const unlisten = await ipc.onVideoProgress((p) => {
      setProgress(p);
    });

    try {
      await ipc.exportVideo({
        audio_path: lastExportedAudioPath,
        output_path: selectedPath,
        style: videoStyle,
        width: 1280,
        height: 720,
        fg_color: videoFgColor,
        bg_color: videoBgColor,
        bg_image_path: videoBgImage,
        fps: 30,
      });
      setDone(true);
      setOutputPath(selectedPath);
    } catch (e) {
      setError(extractErrorMessage(e));
    } finally {
      unlisten();
      setExporting(false);
    }
  };

  const handleCancel = async () => {
    await ipc.cancelVideoExport();
    setExporting(false);
    setProgress(null);
  };

  return (
    <>
      {/* Colors */}
      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-1.5">
          <Label className="text-xs">{t('export.videoFgColor')}</Label>
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">#</span>
            <Input
              className="flex-1 h-9"
              value={videoFgColor}
              onChange={(e) => {
                onFgColorChange(e.target.value.replace(/[^0-9a-fA-F]/g, '').slice(0, 6));
              }}
              maxLength={6}
            />
            <div className="w-9 h-9 rounded-md border shrink-0" style={{ backgroundColor: `#${videoFgColor}` }} />
          </div>
        </div>
        <div className="space-y-1.5">
          <Label className="text-xs">{t('export.videoBgColor')}</Label>
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">#</span>
            <Input
              className="flex-1 h-9"
              value={videoBgColor}
              onChange={(e) => {
                onBgColorChange(e.target.value.replace(/[^0-9a-fA-F]/g, '').slice(0, 6));
              }}
              maxLength={6}
            />
            <div className="w-9 h-9 rounded-md border shrink-0" style={{ backgroundColor: `#${videoBgColor}` }} />
          </div>
        </div>
      </div>

      {/* Background image */}
      <div className="space-y-1.5">
        <Label className="text-xs">{t('export.videoBgImage')}</Label>
        <div className="flex gap-2 items-center">
          <Input
            className="flex-1 h-9"
            placeholder={t('export.videoBgImagePlaceholder')}
            value={videoBgImage ?? ''}
            readOnly
          />
          <Button variant="outline" size="icon" className="h-9 w-9 shrink-0" onClick={() => void handleBgImageBrowse()}>
            <Image className="h-4 w-4" />
          </Button>
          {videoBgImage && (
            <Button
              variant="ghost"
              size="icon"
              className="h-9 w-9 shrink-0"
              onClick={() => {
                setVideoBgImage(null);
              }}
            >
              ✕
            </Button>
          )}
        </div>
        <p className="text-[11px] text-muted-foreground">{t('export.videoBgImageHint')}</p>
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

      {/* Error */}
      {error && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>{t('export.videoExportFailed')}</AlertTitle>
          <AlertDescription className="text-xs">{error}</AlertDescription>
        </Alert>
      )}

      {/* Success */}
      {done && outputPath && (
        <Alert>
          <CheckCircle className="h-4 w-4 text-green-500" />
          <AlertTitle>{t('export.videoExportSuccess')}</AlertTitle>
          <AlertDescription className="text-xs mt-1 space-y-2">
            <p className="text-muted-foreground truncate">{outputPath}</p>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                className="gap-1.5 h-7 text-xs"
                onClick={() => void openPath(outputPath)}
              >
                <ExternalLink className="h-3 w-3" />
                {t('export.openVideo')}
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="gap-1.5 h-7 text-xs"
                onClick={() => void revealItemInDir(outputPath)}
              >
                <Folder className="h-3 w-3" />
                {t('export.openFolder')}
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      )}

      {/* Export button */}
      <div className="flex gap-2">
        <Button
          className="flex-1 gap-2"
          variant="secondary"
          onClick={() => void handleExport()}
          disabled={exporting || !audioReady}
        >
          {exporting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Video className="h-4 w-4" />}
          {exporting ? t('export.videoExporting') : t('export.videoExportButton')}
        </Button>
        {exporting && (
          <Button
            variant="outline"
            className="gap-1.5 text-destructive border-destructive/30 hover:bg-destructive/10"
            onClick={() => void handleCancel()}
          >
            {t('export.cancelVideo')}
          </Button>
        )}
      </div>
    </>
  );
}
