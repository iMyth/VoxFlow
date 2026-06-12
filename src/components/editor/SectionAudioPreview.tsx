import { Volume2, AlertCircle, CheckCircle2, Sparkles } from 'lucide-react';
import { useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import { useProjectStore } from '../../store/projectStore';
import { useSectionVideoStore } from '../../store/sectionVideoStore';
import { useToastStore } from '../../store/toastStore';
import { Button } from '../ui/button';

import type { ScriptLine } from '../../types';

interface SectionAudioPreviewProps {
  sectionId: string;
  lines: ScriptLine[];
}

export default function SectionAudioPreview({ sectionId, lines }: SectionAudioPreviewProps) {
  const { t } = useTranslation();
  const audioFragments = useProjectStore((s) => s.currentProject?.audio_fragments ?? []);
  const audioReady = useSectionVideoStore((s) => s.audioReady[sectionId] ?? false);
  const setAudioReady = useSectionVideoStore((s) => s.setAudioReady);
  const addToast = useToastStore((s) => s.addToast);

  // Count lines with audio
  const audioStats = useMemo(() => {
    const audioLineIds = new Set(audioFragments.map((af) => af.line_id));
    const linesWithAudio = lines.filter((line) => audioLineIds.has(line.id));
    const missingAudio = lines.filter((line) => !audioLineIds.has(line.id));

    return {
      total: lines.length,
      withAudio: linesWithAudio.length,
      missing: missingAudio.length,
      missingLineIds: missingAudio.map((l) => l.id),
      isComplete: lines.length > 0 && linesWithAudio.length === lines.length,
    };
  }, [lines, audioFragments]);

  // Auto-update audioReady state
  useEffect(() => {
    setAudioReady(sectionId, audioStats.isComplete);
  }, [sectionId, audioStats.isComplete, setAudioReady]);

  const handleGenerateMissing = () => {
    if (audioStats.missingLineIds.length === 0) {
      addToast(t('editor.audioPreview.allReady'), 'info');
      return;
    }

    // TODO: Call batch TTS generation for missing lines in this section
    addToast(t('editor.audioPreview.generating'), 'info');
    // This will be implemented in Phase 5 with batch operations
  };

  if (lines.length === 0) {
    return null;
  }

  return (
    <div className="rounded-lg border border-border/60 bg-card/50 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Volume2 className="h-4 w-4 text-muted-foreground" />
          <span className="text-sm font-medium text-foreground">{t('editor.audioPreview.title')}</span>
        </div>
        {audioReady ? (
          <div className="flex items-center gap-1.5 text-xs text-green-600 dark:text-green-400">
            <CheckCircle2 className="h-3.5 w-3.5" />
            <span>{t('editor.audioPreview.ready')}</span>
          </div>
        ) : (
          <div className="flex items-center gap-1.5 text-xs text-orange-600 dark:text-orange-400">
            <AlertCircle className="h-3.5 w-3.5" />
            <span>{t('editor.audioPreview.incomplete')}</span>
          </div>
        )}
      </div>

      <div className="text-xs text-muted-foreground">
        {t('editor.audioPreview.stats', {
          withAudio: audioStats.withAudio,
          total: audioStats.total,
        })}
      </div>

      {!audioReady && (
        <Button
          variant="outline"
          size="sm"
          className="w-full gap-2"
          onClick={handleGenerateMissing}
          disabled={audioStats.missing === 0}
        >
          <Sparkles className="h-3.5 w-3.5" />
          {t('editor.audioPreview.generateMissing', { count: audioStats.missing })}
        </Button>
      )}
    </div>
  );
}
