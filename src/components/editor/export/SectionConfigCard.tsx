import { AlertTriangle, Loader2, Play, RefreshCw, Sparkles, Wand2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '../../ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '../../ui/card';
import { Label } from '../../ui/label';
import { Progress } from '../../ui/progress';
import { Tabs, TabsList, TabsTrigger } from '../../ui/tabs';

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

type TemplateId = 'minimal-subtitle' | 'dialogue-cards' | 'chapter-sections';

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

  const templates: { id: TemplateId; name: string; desc: string }[] = [
    {
      id: 'minimal-subtitle',
      name: t('export.hyperframesTemplateMinimalSubtitle'),
      desc: t('export.hyperframesTemplateMinimalSubtitleDesc'),
    },
    {
      id: 'dialogue-cards',
      name: t('export.hyperframesTemplateDialogueCards'),
      desc: t('export.hyperframesTemplateDialogueCardsDesc'),
    },
    {
      id: 'chapter-sections',
      name: t('export.hyperframesTemplateChapterSections'),
      desc: t('export.hyperframesTemplateChapterSectionsDesc'),
    },
  ];

  const handleModeChange = (mode: string) => {
    onConfigChange({ ...config, mode: mode as SectionStyleConfig['mode'] });
  };

  const handleTemplateChange = (templateId: TemplateId) => {
    onConfigChange({ ...config, template: templateId });
  };

  const handlePromptChange = (prompt: string) => {
    if (prompt.length <= 500) {
      onConfigChange({ ...config, ai_prompt: prompt });
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
        {/* Mode toggle tabs */}
        <div className="space-y-1.5">
          <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
            {t('export.sectionMode')}
          </Label>
          <Tabs value={config.mode} onValueChange={handleModeChange}>
            <TabsList className="w-full">
              <TabsTrigger value="template" className="flex-1">
                {t('export.sectionModeTemplate')}
              </TabsTrigger>
              <TabsTrigger value="ai" className="flex-1">
                <Sparkles className="h-3.5 w-3.5 mr-1" />
                {t('export.sectionModeAi')}
              </TabsTrigger>
              <TabsTrigger value="agent" className="flex-1">
                <Wand2 className="h-3.5 w-3.5 mr-1" />
                {t('export.sectionModeAgent')}
              </TabsTrigger>
            </TabsList>
          </Tabs>
        </div>

        {/* Template picker */}
        {config.mode === 'template' && (
          <div className="space-y-1.5">
            <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
              {t('export.sectionSelectTemplate')}
            </Label>
            <div className="grid gap-2">
              {templates.map((tmpl) => (
                <button
                  key={tmpl.id}
                  type="button"
                  className={`flex items-start gap-3 rounded-lg border p-3 text-left transition-colors ${
                    (config.template ?? 'minimal-subtitle') === tmpl.id
                      ? 'border-primary bg-primary/5'
                      : 'border-border hover:border-primary/50 hover:bg-muted/50'
                  }`}
                  onClick={() => {
                    handleTemplateChange(tmpl.id);
                  }}
                >
                  <div
                    className={`mt-0.5 h-3 w-3 rounded-full border-2 shrink-0 ${
                      (config.template ?? 'minimal-subtitle') === tmpl.id
                        ? 'border-primary bg-primary'
                        : 'border-muted-foreground/40'
                    }`}
                  />
                  <div>
                    <div className="text-sm font-medium">{tmpl.name}</div>
                    <div className="text-xs text-muted-foreground">{tmpl.desc}</div>
                  </div>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* AI prompt */}
        {config.mode === 'ai' && (
          <div className="space-y-2">
            <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
              {t('export.sectionAiPromptLabel')}
            </Label>
            <textarea
              className="w-full min-h-[80px] rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y"
              placeholder={t('export.sectionAiPromptPlaceholder')}
              value={config.ai_prompt ?? ''}
              maxLength={500}
              onChange={(e) => {
                handlePromptChange(e.target.value);
              }}
            />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>{t('export.sectionAiHint')}</span>
              <span>{(config.ai_prompt ?? '').length}/500</span>
            </div>
          </div>
        )}

        {/* Agent prompt */}
        {config.mode === 'agent' && (
          <div className="space-y-2">
            <Label className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
              {t('export.sectionAgentPromptLabel')}
            </Label>
            <textarea
              className="w-full min-h-[80px] rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y"
              placeholder={t('export.sectionAgentPromptPlaceholder')}
              value={config.ai_prompt ?? ''}
              maxLength={500}
              onChange={(e) => {
                handlePromptChange(e.target.value);
              }}
            />
            <div className="flex justify-between text-xs text-muted-foreground">
              <span>{t('export.sectionAgentHint')}</span>
              <span>{(config.ai_prompt ?? '').length}/500</span>
            </div>
          </div>
        )}

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
