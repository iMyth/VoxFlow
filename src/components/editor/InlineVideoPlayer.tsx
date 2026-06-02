import { convertFileSrc } from '@tauri-apps/api/core';
import { Play, Pause, Volume2, VolumeX, RotateCcw, AlertCircle } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { useVideoPlayback } from '../../contexts/VideoPlaybackContext';
import { Button } from '../ui/button';

interface InlineVideoPlayerProps {
  /** 视频文件的本地路径 */
  videoPath: string;
  /** 所属 section 的 ID */
  sectionId: string;
  /** 视频时长（毫秒） */
  durationMs: number;
  /** 视频加载失败时的回调 */
  onError?: () => void;
  /** 用于 cache-busting 的版本号（每次重新生成递增） */
  version?: number;
  /** 重新生成视频的回调 */
  onRegenerate?: () => void;
}

function formatTime(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${min}:${String(sec).padStart(2, '0')}`;
}

export default function InlineVideoPlayer({
  videoPath,
  sectionId,
  durationMs,
  onError,
  version,
  onRegenerate,
}: InlineVideoPlayerProps) {
  const { playingSectionId, requestPlay, notifyPause } = useVideoPlayback();

  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  const [isInViewport, setIsInViewport] = useState(false);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isMuted, setIsMuted] = useState(true);
  const [hasEnded, setHasEnded] = useState(false);
  const [hasError, setHasError] = useState(false);
  const [currentTimeMs, setCurrentTimeMs] = useState(0);
  const [videoDurationMs, setVideoDurationMs] = useState(durationMs);

  // Generate video src URL with cache-busting
  const videoSrc = convertFileSrc(videoPath) + (version != null ? `?v=${version}` : '');

  // IntersectionObserver for viewport-aware lazy loading
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        setIsInViewport(entry.isIntersecting);
      },
      { threshold: 0 }
    );

    observer.observe(container);
    return () => {
      observer.disconnect();
    };
  }, []);

  // Pause when another section starts playing or when playingSectionId becomes null (pauseAll)
  useEffect(() => {
    if (playingSectionId !== sectionId && isPlaying) {
      const video = videoRef.current;
      if (video) {
        video.pause();
      }
    }
  }, [playingSectionId, sectionId, isPlaying]);

  // Cleanup on unmount: explicitly pause video to ensure cleanup within 500ms of navigation
  useEffect(() => {
    const video = videoRef.current;
    return () => {
      if (video && !video.paused) {
        video.pause();
      }
    };
  }, []);

  // Sync isPlaying state with video element events
  const handlePlay = useCallback(() => {
    setIsPlaying(true);
    setHasEnded(false);
    requestPlay(sectionId);
  }, [requestPlay, sectionId]);

  const handlePause = useCallback(() => {
    setIsPlaying(false);
    notifyPause(sectionId);
  }, [notifyPause, sectionId]);

  const handleEnded = useCallback(() => {
    setIsPlaying(false);
    setHasEnded(true);
    notifyPause(sectionId);
  }, [notifyPause, sectionId]);

  const handleTimeUpdate = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    setCurrentTimeMs(video.currentTime * 1000);
  }, []);

  const handleLoadedMetadata = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    setVideoDurationMs(video.duration * 1000);
  }, []);

  const handleVideoError = useCallback(() => {
    setHasError(true);
    setIsPlaying(false);
    onError?.();
  }, [onError]);

  // Play/pause toggle
  const togglePlayPause = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;

    if (hasEnded) {
      // Replay from start
      video.currentTime = 0;
      setHasEnded(false);
      void video.play();
    } else if (isPlaying) {
      video.pause();
    } else {
      void video.play();
    }
  }, [isPlaying, hasEnded]);

  // Mute toggle
  const toggleMute = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = !isMuted;
    setIsMuted(!isMuted);
  }, [isMuted]);

  // Progress scrubber seek
  const handleSeek = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const video = videoRef.current;
      if (!video) return;
      const ms = Number(e.target.value);
      video.currentTime = ms / 1000;
      setCurrentTimeMs(ms);
      if (hasEnded) {
        setHasEnded(false);
      }
    },
    [hasEnded]
  );

  // Space key for play/pause toggle
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === ' ' || e.code === 'Space') {
        e.preventDefault();
        togglePlayPause();
      }
    },
    [togglePlayPause]
  );

  const progress = videoDurationMs > 0 ? (currentTimeMs / videoDurationMs) * 100 : 0;

  // Error state
  if (hasError) {
    return (
      <div
        ref={containerRef}
        className="flex flex-col items-center justify-center gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-6"
      >
        <AlertCircle className="h-8 w-8 text-destructive" />
        <p className="text-sm text-destructive">视频加载失败</p>
        {onRegenerate && (
          <Button variant="outline" size="sm" onClick={onRegenerate}>
            重新生成
          </Button>
        )}
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="flex flex-col gap-1 rounded-lg border bg-muted/40 overflow-hidden"
      tabIndex={0}
      onKeyDown={handleKeyDown}
    >
      {/* Video element - only rendered when in viewport */}
      {isInViewport && (
        <div className="relative w-full max-h-[200px] aspect-video bg-black">
          <video
            ref={videoRef}
            key={`${videoPath}-${version ?? 0}`}
            src={videoSrc}
            className="w-full h-full object-contain"
            preload="metadata"
            muted={isMuted}
            onPlay={handlePlay}
            onPause={handlePause}
            onEnded={handleEnded}
            onTimeUpdate={handleTimeUpdate}
            onLoadedMetadata={handleLoadedMetadata}
            onError={handleVideoError}
          />
        </div>
      )}

      {/* Custom controls */}
      <div className="flex items-center gap-2 px-3 py-2">
        {/* Play/Pause/Replay button */}
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0 rounded-full"
          onClick={togglePlayPause}
          aria-label={hasEnded ? 'Replay' : isPlaying ? 'Pause' : 'Play'}
        >
          {hasEnded ? (
            <RotateCcw className="h-3.5 w-3.5" />
          ) : isPlaying ? (
            <Pause className="h-3.5 w-3.5" />
          ) : (
            <Play className="h-3.5 w-3.5" />
          )}
        </Button>

        {/* Progress scrubber + time display */}
        <div className="flex items-center gap-1.5 flex-1 min-w-0">
          <span className="text-xs text-muted-foreground tabular-nums w-9 text-right shrink-0">
            {formatTime(currentTimeMs)}
          </span>
          <input
            type="range"
            min={0}
            max={videoDurationMs || 1}
            step={10}
            value={Math.min(currentTimeMs, videoDurationMs)}
            onChange={handleSeek}
            className="flex-1 min-w-0 accent-primary cursor-pointer [&::-webkit-slider-runnable-track]:rounded-full [&::-webkit-slider-runnable-track]:h-1 [&::-webkit-slider-runnable-track]:bg-muted/50 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:size-4 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:shadow-md [&::-webkit-slider-thumb]:-mt-1.5 [&::-moz-range-track]:rounded-full [&::-moz-range-track]:h-1 [&::-moz-range-track]:bg-muted/50 [&::-moz-range-thumb]:appearance-none [&::-moz-range-thumb]:size-4 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-primary [&::-moz-range-thumb]:border-0 [&::-moz-range-thumb]:shadow-md"
            style={{
              background: `linear-gradient(to right, hsl(var(--primary)) ${progress}%, hsl(var(--muted) / 0.3) ${progress}%)`,
              WebkitAppearance: 'none',
              appearance: 'none',
              borderRadius: '9999px',
            }}
          />
          <span className="text-xs text-muted-foreground tabular-nums w-9 shrink-0">{formatTime(videoDurationMs)}</span>
        </div>

        {/* Mute/Unmute toggle */}
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 shrink-0 rounded-full"
          onClick={toggleMute}
          aria-label={isMuted ? 'Unmute' : 'Mute'}
        >
          {isMuted ? <VolumeX className="h-3.5 w-3.5" /> : <Volume2 className="h-3.5 w-3.5" />}
        </Button>
      </div>
    </div>
  );
}
