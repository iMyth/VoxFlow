import { listen } from '@tauri-apps/api/event';
import { appDataDir } from '@tauri-apps/api/path';
import { ChevronDown, Loader2, RefreshCw, Sparkles, Wand2, AlertCircle } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import InlineVideoPlayer from './InlineVideoPlayer';
import { extractErrorMessage } from '../../lib/extractErrorMessage';
import { checkSectionVideoExists, generateSectionVideo, onSectionVideoProgress } from '../../lib/ipc';
import { useScriptStore } from '../../store/scriptStore';
import { useSectionVideoStore } from '../../store/sectionVideoStore';
import { Button } from '../ui/button';
import { Label } from '../ui/label';
import { Progress } from '../ui/progress';
import { Switch } from '../ui/switch';

import type { ScriptSection, SectionStyleConfig } from '../../types';
import type { UnlistenFn } from '@tauri-apps/api/event';

interface SectionVideoPanelProps {
  section: ScriptSection;
  projectId: string;
}

const STATUS_DOT_COLORS: Record<string, string> = {
  not_started: 'bg-gray-400',
  generating: 'bg-blue-500',
  completed: 'bg-green-500',
  failed: 'bg-red-500',
};

export default function SectionVideoPanel({ section, projectId }: SectionVideoPanelProps) {
  const { t } = useTranslation();

  const rawConfig = useSectionVideoStore((s) => s.configs[section.id]);
  const config = useMemo(
    () => rawConfig ?? ({ mode: 'agent' as const, user_prompt: '', useGlobalStyle: true } satisfies SectionStyleConfig),
    [rawConfig]
  );
  const status = useSectionVideoStore((s) => s.statuses[section.id]) ?? { state: 'not_started' as const };
  const audioReady = useSectionVideoStore((s) => s.audioReady[section.id] ?? false);
  const setConfig = useSectionVideoStore((s) => s.setConfig);
  const setStatus = useSectionVideoStore((s) => s.setStatus);
  const setVideoReady = useSectionVideoStore((s) => s.setVideoReady);

  const globalVideoStyle = useScriptStore((s) => s.globalVideoStyle);

  // Local state
  const [isExpanded, setIsExpanded] = useState(status.state === 'completed');
  const [videoVersion, setVideoVersion] = useState(0);
  const [videoPath, setVideoPath] = useState<string | null>(null);
  const [isGenerating, setIsGenerating] = useState(false);

  // Track whether user has manually toggled (overrides auto-expand)
  const userToggledRef = useRef(false);
  // Track previous status state for transition detection
  const prevStatusStateRef = useRef(status.state);

  // Resolve video path on mount and when projectId/sectionId changes
  useEffect(() => {
    let cancelled = false;
    void appDataDir().then((dir) => {
      if (!cancelled) {
        const base = dir.endsWith('/') ? dir : `${dir}/`;
        setVideoPath(`${base}projects/${projectId}/export/sections/${section.id}/output.mp4`);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, section.id]);

  // Check if video already exists on mount and restore status
  useEffect(() => {
    if (status.state !== 'not_started') return;

    void checkSectionVideoExists(projectId, section.id).then((exists) => {
      if (exists) {
        setStatus(section.id, {
          state: 'completed',
          duration_ms: 0,
          file_size_bytes: 0,
        });
        setVideoReady(section.id, true);
        if (!userToggledRef.current) {
          setIsExpanded(true);
        }
      }
    });
  }, [projectId, section.id, status.state, setStatus, setVideoReady]);

  // Auto-expand when status transitions to completed
  useEffect(() => {
    if (prevStatusStateRef.current !== 'completed' && status.state === 'completed') {
      // Increment video version for cache-busting
      setVideoVersion((v) => v + 1);
      setIsGenerating(false);

      // Auto-expand only if user hasn't manually toggled
      if (!userToggledRef.current) {
        setIsExpanded(true);
      }
    }

    if (status.state === 'failed') {
      setIsGenerating(false);
    }

    prevStatusStateRef.current = status.state;
  }, [status.state]);

  // Listen for section-video-progress events for this section
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    void onSectionVideoProgress((progress) => {
      if (progress.section_id === section.id) {
        setStatus(section.id, {
          state: 'generating',
          percent: progress.percent,
          stage: progress.stage,
        });
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    // Listen for section-video-complete
    void listen<{ section_id: string; video_path: string; duration_ms: number; file_size_bytes: number }>(
      'section-video-complete',
      (event) => {
        if (event.payload.section_id === section.id) {
          setStatus(section.id, {
            state: 'completed',
            duration_ms: event.payload.duration_ms,
            file_size_bytes: event.payload.file_size_bytes,
          });
        }
      }
    ).then((unlisten) => unlisteners.push(unlisten));

    // Listen for section-video-failed
    void listen<{ section_id: string; error: string }>('section-video-failed', (event) => {
      if (event.payload.section_id === section.id) {
        setStatus(section.id, {
          state: 'failed',
          error: event.payload.error,
        });
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [section.id, setStatus]);

  // Toggle collapse/expand
  const handleToggle = useCallback(() => {
    userToggledRef.current = true;
    setIsExpanded((prev) => !prev);
  }, []);

  // Handle generate/regenerate
  const handleGenerate = useCallback(async () => {
    if (isGenerating || !audioReady) return;
    setIsGenerating(true);
    setStatus(section.id, { state: 'generating', percent: 0, stage: 'starting' });

    // Determine effective style
    const effectiveStyle = config.useGlobalStyle !== false ? globalVideoStyle : config.customStyle;

    // Build the config to send to backend
    const generationConfig: SectionStyleConfig = {
      mode: config.mode,
      user_prompt: effectiveStyle || config.user_prompt,
    };

    try {
      const result = await generateSectionVideo(projectId, section.id, generationConfig);
      setStatus(section.id, {
        state: 'completed',
        duration_ms: result.duration_ms,
        file_size_bytes: result.file_size_bytes,
      });
      setVideoReady(section.id, true);
    } catch (e) {
      setStatus(section.id, {
        state: 'failed',
        error: extractErrorMessage(e).slice(0, 200),
      });
      setVideoReady(section.id, false);
    }
  }, [isGenerating, audioReady, projectId, section.id, config, globalVideoStyle, setStatus, setVideoReady]);

  const handlePromptChange = useCallback(
    (prompt: string) => {
      if (prompt.length <= 500) {
        setConfig(section.id, { ...config, user_prompt: prompt });
      }
    },
    [section.id, config, setConfig]
  );

  const handleStyleOverrideToggle = useCallback(
    (useGlobal: boolean) => {
      setConfig(section.id, { ...config, useGlobalStyle: useGlobal });
    },
    [section.id, config, setConfig]
  );

  const handleCustomStyleChange = useCallback(
    (customStyle: string) => {
      if (customStyle.length <= 500) {
        setConfig(section.id, { ...config, customStyle });
      }
    },
    [section.id, config, setConfig]
  );

  // Handle video error (trigger regenerate)
  const handleVideoError = useCallback(() => {
    // Video load failed — user can click regenerate
  }, []);

  const statusState = status.state;
  const statusDotColor = STATUS_DOT_COLORS[statusState] ?? 'bg-gray-400';

  return (
    <div className="rounded-lg border border-border/60 bg-card/50 overflow-hidden">
      {/* Header — always visible */}
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-muted/30 transition-colors"
        onClick={handleToggle}
      >
        <span className={`h-2 w-2 rounded-full shrink-0 ${statusDotColor}`} />
        <span className="text-xs text-muted-foreground truncate flex-1">Agent</span>
        {!audioReady && <AlertCircle className="h-3 w-3 text-orange-500" />}
        <ChevronDown
          className={`h-3.5 w-3.5 text-muted-foreground transition-transform duration-300 ${
            isExpanded ? 'rotate-0' : '-rotate-90'
          }`}
        />
      </button>

      {/* Expandable content with animation */}
      <div
        className={`transition-all duration-300 ease-in-out overflow-hidden ${
          isExpanded ? 'max-h-[800px] opacity-100' : 'max-h-0 opacity-0'
        }`}
      >
        <div className="px-3 pb-3 space-y-3 border-t border-border/40">
          {/* Audio not ready warning */}
          {!audioReady && (
            <div className="flex items-start gap-2 rounded-md bg-orange-50 dark:bg-orange-950/20 border border-orange-200 dark:border-orange-800 px-2.5 py-2">
              <AlertCircle className="h-3 w-3 text-orange-500 shrink-0 mt-0.5" />
              <span className="text-xs text-orange-700 dark:text-orange-300">{t('export.sectionAudioNotReady')}</span>
            </div>
          )}

          {/* Style override toggle */}
          <div className="flex items-center justify-between pt-3">
            <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
              {t('export.sectionStyleOverride')}
            </Label>
            <div className="flex items-center gap-2">
              <span className="text-xs text-muted-foreground">
                {config.useGlobalStyle !== false ? t('export.sectionStyleGlobal') : t('export.sectionStyleCustom')}
              </span>
              <Switch
                checked={config.useGlobalStyle === false}
                onCheckedChange={(checked) => {
                  handleStyleOverrideToggle(!checked);
                }}
              />
            </div>
          </div>

          {/* User prompt input */}
          <div className="space-y-1.5">
            <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
              <Wand2 className="h-3 w-3 inline mr-1" />
              {config.useGlobalStyle === false
                ? t('export.sectionCustomStyleLabel')
                : t('export.sectionAgentPromptLabel')}
            </Label>
            {config.useGlobalStyle === false ? (
              <textarea
                className="w-full min-h-15 rounded-md border border-input bg-background px-2.5 py-1.5 text-xs placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y"
                placeholder={t('export.sectionCustomStylePlaceholder')}
                value={config.customStyle ?? ''}
                maxLength={500}
                onChange={(e) => {
                  handleCustomStyleChange(e.target.value);
                }}
                disabled={!audioReady}
              />
            ) : (
              <textarea
                className="w-full min-h-15 rounded-md border border-input bg-background px-2.5 py-1.5 text-xs placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y"
                placeholder={t('export.sectionAgentPromptPlaceholder')}
                value={config.user_prompt ?? ''}
                maxLength={500}
                onChange={(e) => {
                  handlePromptChange(e.target.value);
                }}
                disabled={!audioReady}
              />
            )}
            <div className="flex justify-between text-[10px] text-muted-foreground">
              <span>
                {config.useGlobalStyle === false ? t('export.sectionCustomStyleHint') : t('export.sectionAgentHint')}
              </span>
              <span>
                {(config.useGlobalStyle === false ? (config.customStyle ?? '') : (config.user_prompt ?? '')).length}/500
              </span>
            </div>
          </div>

          {/* Progress indicator when generating */}
          {statusState === 'generating' && status.state === 'generating' && (
            <div className="rounded-md bg-muted/50 p-2.5 space-y-1.5">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Loader2 className="h-3 w-3 animate-spin" />
                <span className="truncate">{status.stage}</span>
                <span className="ml-auto font-mono text-[10px]">{Math.round(status.percent)}%</span>
              </div>
              <Progress value={status.percent} className="h-1" />
            </div>
          )}

          {/* Error message when failed */}
          {statusState === 'failed' && status.state === 'failed' && (
            <div className="flex items-start gap-2 rounded-md bg-destructive/10 border border-destructive/20 px-2.5 py-2">
              <span className="text-xs text-destructive line-clamp-3 flex-1">{status.error}</span>
              <Button
                size="sm"
                variant="ghost"
                className="shrink-0 text-xs h-6 px-2 text-destructive hover:text-destructive"
                onClick={() => void handleGenerate()}
              >
                重试
              </Button>
            </div>
          )}

          {/* Generate / Regenerate button */}
          <Button
            size="sm"
            variant="secondary"
            className="w-full gap-1.5 text-xs"
            onClick={() => void handleGenerate()}
            disabled={isGenerating || statusState === 'generating'}
          >
            {statusState === 'generating' ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : statusState === 'completed' || statusState === 'failed' ? (
              <RefreshCw className="h-3 w-3" />
            ) : (
              <Sparkles className="h-3 w-3" />
            )}
            {statusState === 'not_started'
              ? t('export.sectionGenerate')
              : statusState === 'generating'
                ? t('export.sectionGenerating')
                : t('export.sectionRegenerate')}
          </Button>

          {/* Inline video player when completed */}
          {statusState === 'completed' && status.state === 'completed' && videoPath && (
            <InlineVideoPlayer
              videoPath={videoPath}
              sectionId={section.id}
              durationMs={status.duration_ms}
              version={videoVersion}
              onError={handleVideoError}
              onRegenerate={() => void handleGenerate()}
            />
          )}
        </div>
      </div>
    </div>
  );
}
