import { AlertTriangle, Loader2, Play, RefreshCw, Sparkles, Wand2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '../../ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '../../ui/card';
import { Label } from '../../ui/label';
import { Progress } from '../../ui/progress';

import type { ScriptSection, SectionStatus, SectionStyleConfig } from '../../../types';

interface SectionConfigCardProps {
  section: ScriptSection;
  config: SectionStyleConfig;
  status: SectionStatus;
  onConfigChange: (config: SectionStyleConfig) => void;
  onGenerate: () => void;
  onPreview: () => void;
}

const STATUS_DOT_COLORS: Record<SectionStatus['state'], string> = {
  not_started: 'bg-gray-400',
  generating: 'bg-blue-500',
  completed: 'bg-green-500',
  failed: 'bg-red-500',
};

function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024 * 1024) {
    return `${Math.round(bytes / 1024)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function SectionConfigCard({
  section,
  config,
  status,
  onConfigChange,
  onGenerate,
  onPreview,
}: SectionConfigCardProps) {
  const { t } = useTranslation();

  const handlePromptChange = (prompt: string) => {
    if (prompt.length <= 500) {
      onConfigChange({ ...config, user_prompt: prompt });
    }
  };

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <span className={`h-2.5 w-2.5 rounded-full shrink-0 ${STATUS_DOT_COLORS[status.state]}`} />
          <span className="truncate">{section.title}</span>
        </CardTitle>
      </CardHeader>

      <CardContent className="space-y-3">
        {/* Agent prompt */}
        <div className="space-y-2">
          <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
            <Wand2 className="h-3.5 w-3.5 inline mr-1" />
            {t('export.sectionAgentPromptLabel')}
          </Label>
          <textarea
            className="w-full min-h-20 rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y"
            placeholder={t('export.sectionAgentPromptPlaceholder')}
            value={config.user_prompt ?? ''}
            maxLength={500}
            onChange={(e) => {
              handlePromptChange(e.target.value);
            }}
          />
          <div className="flex justify-between text-xs text-muted-foreground">
            <span>{t('export.sectionAgentHint')}</span>
            <span>{(config.user_prompt ?? '').length}/500</span>
          </div>
        </div>

        {/* Progress bar when generating */}
        {status.state === 'generating' && (
          <div className="rounded-lg bg-muted/50 p-3 space-y-2">
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              <span>{status.stage}</span>
              <span className="ml-auto font-mono">{Math.round(status.percent)}%</span>
            </div>
            <Progress value={status.percent} className="h-1.5" />
          </div>
        )}

        {/* Duration and file size when completed */}
        {status.state === 'completed' && (
          <div className="flex items-center gap-3 text-xs text-muted-foreground rounded-lg bg-muted/50 px-3 py-2">
            <span>
              {t('export.sectionDuration')}: {formatDuration(status.duration_ms)}
            </span>
            <span className="text-border">|</span>
            <span>
              {t('export.sectionFileSize')}: {formatFileSize(status.file_size_bytes)}
            </span>
          </div>
        )}

        {/* Error message when failed */}
        {status.state === 'failed' && (
          <div className="flex items-start gap-2 rounded-lg bg-destructive/10 border border-destructive/20 px-3 py-2">
            <AlertTriangle className="h-3.5 w-3.5 text-destructive shrink-0 mt-0.5" />
            <span className="text-xs text-destructive line-clamp-3">{status.error}</span>
          </div>
        )}

        {/* Action buttons */}
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="secondary"
            className="gap-1.5 flex-1"
            onClick={onGenerate}
            disabled={status.state === 'generating'}
          >
            {status.state === 'generating' ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Sparkles className="h-3.5 w-3.5" />
            )}
            {status.state === 'not_started'
              ? t('export.sectionGenerate')
              : status.state === 'generating'
                ? t('export.sectionGenerating')
                : t('export.sectionRegenerate')}
          </Button>

          <Button
            size="sm"
            variant="outline"
            className="gap-1.5"
            onClick={onPreview}
            disabled={status.state !== 'completed'}
          >
            <Play className="h-3.5 w-3.5" />
            {t('export.sectionPreview')}
          </Button>

          {status.state !== 'not_started' && status.state !== 'generating' && (
            <Button size="sm" variant="ghost" className="gap-1.5" onClick={onGenerate}>
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
