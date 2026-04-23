import { convertFileSrc } from '@tauri-apps/api/core';
import { Play, Pause } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useAudioStore } from '../../store/audioStore';
import { Button } from '../ui/button';

interface AudioPlayerProps {
  filePath: string;
}

export default function AudioPlayer({ filePath }: AudioPlayerProps) {
  const { t } = useTranslation();
  const playingPath = useAudioStore((s) => s.playingPath);
  const setPlayingPath = useAudioStore((s) => s.setPlayingPath);
  const isPlaying = playingPath === filePath;

  const audioRef = useRef<HTMLAudioElement>(null);
  const [positionMs, setPositionMs] = useState(0);
  const [durationMs, setDurationMs] = useState(0);
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    setSrc(convertFileSrc(filePath));
  }, [filePath]);

  const toggle = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    if (isPlaying) {
      audio.pause();
      setPlayingPath(null);
    } else {
      void audio.play();
      setPlayingPath(filePath);
    }
  }, [isPlaying, filePath, setPlayingPath]);

  const handleTimeUpdate = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    setPositionMs(audio.currentTime * 1000);
  }, []);

  const handleLoadedMetadata = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    setDurationMs(audio.duration * 1000);
  }, []);

  const handleEnded = useCallback(() => {
    setPlayingPath(null);
    setPositionMs(0);
  }, [setPlayingPath]);

  const handleSeek = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const audio = audioRef.current;
    if (!audio) return;
    const ms = Number(e.target.value);
    audio.currentTime = ms / 1000;
    setPositionMs(ms);
  }, []);

  const formatTime = (ms: number) => {
    const totalSec = Math.floor(ms / 1000);
    const min = Math.floor(totalSec / 60);
    const sec = totalSec % 60;
    return `${min}:${String(sec).padStart(2, '0')}`;
  };

  const progress = durationMs > 0 ? (positionMs / durationMs) * 100 : 0;

  return (
    <div className="flex flex-col gap-1 rounded-lg border bg-muted/40 px-3 py-2">
      <audio
        ref={audioRef}
        src={src ?? undefined}
        onTimeUpdate={handleTimeUpdate}
        onLoadedMetadata={handleLoadedMetadata}
        onEnded={handleEnded}
      />
      {/* Play button + slider always on one line, wraps time below */}
      <div className="flex items-center gap-2 flex-wrap">
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0 rounded-full"
          onClick={toggle}
          aria-label={isPlaying ? t('editor.pause') : t('editor.play')}
        >
          {isPlaying ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
        </Button>
        <div className="flex items-center gap-1.5 flex-1 min-w-30">
          <span className="text-xs text-muted-foreground tabular-nums w-9 text-right shrink-0">
            {formatTime(positionMs)}
          </span>
          <input
            type="range"
            min={0}
            max={durationMs || 1}
            step={10}
            value={Math.min(positionMs, durationMs)}
            onChange={handleSeek}
            className="flex-1 min-w-0 accent-primary cursor-pointer [&::-webkit-slider-runnable-track]:rounded-full [&::-webkit-slider-runnable-track]:h-1 [&::-webkit-slider-runnable-track]:bg-muted/50 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:size-4 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:shadow-md [&::-webkit-slider-thumb]:-mt-1.5 [&::-moz-range-track]:rounded-full [&::-moz-range-track]:h-1 [&::-moz-range-track]:bg-muted/50 [&::-moz-range-thumb]:appearance-none [&::-moz-range-thumb]:size-4 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:border-0 [&::-moz-range-thumb]:shadow-md"
            style={{
              background: `linear-gradient(to right, hsl(var(--primary)) ${progress}%, hsl(var(--muted) / 0.3) ${progress}%)`,
              WebkitAppearance: 'none',
              appearance: 'none',
              borderRadius: '9999px',
            }}
          />
          <span className="text-xs text-muted-foreground tabular-nums w-9 shrink-0">{formatTime(durationMs)}</span>
        </div>
      </div>
    </div>
  );
}
