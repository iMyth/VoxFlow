import { Lock, Sparkles } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import HyperframesExport from './HyperframesExport';
import StandardVideoExport from './StandardVideoExport';
import { Label } from '../../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../ui/select';

import type { VideoStyle } from '../../../lib/ipc';

interface VideoExportStepProps {
  audioReady: boolean;
  lastExportedAudioPath: string | null;
}

export default function VideoExportStep({ audioReady, lastExportedAudioPath }: VideoExportStepProps) {
  const { t } = useTranslation();

  const [videoStyle, setVideoStyle] = useState<VideoStyle | 'hyperframes'>('particles');
  const [videoFgColor, setVideoFgColor] = useState('6366f1');
  const [videoBgColor, setVideoBgColor] = useState('0a0a1a');

  const isHyperframes = videoStyle === 'hyperframes';

  const handleStyleChange = (v: string) => {
    const style = v as VideoStyle | 'hyperframes';
    setVideoStyle(style);
    switch (style) {
      case 'fractal':
        setVideoFgColor('e8a838');
        setVideoBgColor('050510');
        break;
      case 'starfield':
        setVideoFgColor('7c9ff5');
        setVideoBgColor('020208');
        break;
      case 'vinyl':
        setVideoFgColor('6366f1');
        setVideoBgColor('1a1a2e');
        break;
      case 'particles':
        setVideoFgColor('6366f1');
        setVideoBgColor('0a0a1a');
        break;
    }
  };

  return (
    <div
      className={`relative rounded-xl border overflow-hidden transition-all duration-300 ${audioReady || isHyperframes ? 'border-border bg-card' : 'border-border/40 bg-muted/20'}`}
    >
      {/* Locked overlay — only for non-hyperframes video styles */}
      {!audioReady && !isHyperframes && (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-background/60 backdrop-blur-[1px]">
          <div className="flex flex-col items-center gap-2 text-center px-4">
            <div className="flex h-10 w-10 items-center justify-center rounded-full bg-muted">
              <Lock className="h-4 w-4 text-muted-foreground" />
            </div>
            <p className="text-xs text-muted-foreground max-w-[200px]">{t('export.videoNeedAudioFirst')}</p>
          </div>
        </div>
      )}

      {/* Step header */}
      <div className="flex items-center gap-3 px-5 py-3 border-b border-border/50 bg-muted/30">
        <div
          className={`flex h-7 w-7 items-center justify-center rounded-full text-xs font-bold ${audioReady || isHyperframes ? 'bg-purple-100 text-purple-700 dark:bg-purple-900/40 dark:text-purple-400' : 'bg-muted text-muted-foreground'}`}
        >
          2
        </div>
        <div className="flex-1">
          <h3 className="text-sm font-semibold">{t('export.videoTitle')}</h3>
        </div>
      </div>

      {/* Step content */}
      <div className="px-5 py-4 space-y-4">
        {/* Style selector */}
        <div className="space-y-1.5">
          <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
            {t('export.videoStyle')}
          </Label>
          <Select value={videoStyle} onValueChange={handleStyleChange}>
            <SelectTrigger className="h-9">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="particles">{t('export.styleParticles')}</SelectItem>
              <SelectItem value="starfield">{t('export.styleStarfield')}</SelectItem>
              <SelectItem value="vinyl">{t('export.styleVinyl')}</SelectItem>
              <SelectItem value="fractal">{t('export.styleFractal')}</SelectItem>
              <SelectItem value="hyperframes">
                <span className="flex items-center gap-1.5">
                  <Sparkles className="h-3.5 w-3.5" />
                  Hyperframes
                </span>
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        {/* Route to the appropriate export sub-component */}
        {isHyperframes ? (
          <HyperframesExport lastExportedAudioPath={lastExportedAudioPath} />
        ) : (
          <StandardVideoExport
            audioReady={audioReady}
            lastExportedAudioPath={lastExportedAudioPath}
            videoStyle={videoStyle}
            videoFgColor={videoFgColor}
            videoBgColor={videoBgColor}
            onFgColorChange={setVideoFgColor}
            onBgColorChange={setVideoBgColor}
          />
        )}
      </div>
    </div>
  );
}
