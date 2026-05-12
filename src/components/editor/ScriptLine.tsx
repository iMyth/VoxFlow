import { GripVertical, Trash2, Volume2, Loader2, AlertCircle, Mic, RotateCcw } from 'lucide-react';
import { useState, useEffect, useCallback, memo } from 'react';
import { useTranslation } from 'react-i18next';

import AudioPlayer from './AudioPlayer';
import AudioRecorder from './AudioRecorder';
import * as ipc from '../../lib/ipc';
import { useCharacterStore } from '../../store/characterStore';
import { useProjectStore } from '../../store/projectStore';
import { useScriptStore } from '../../store/scriptStore';
import { useToastStore } from '../../store/toastStore';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';

import type { ScriptLine, AudioFragment } from '../../types';

interface ScriptLineProps {
  line: ScriptLine;
  index: number;
  totalLines?: number;
  isDragging?: boolean;
  dropPosition?: 'before' | 'after' | null;
  onDragStart?: (lineId: string, pointerId: number) => void;
  onDragMove?: (clientX: number, clientY: number) => void;
  onDragEnd?: () => void;
}

function ScriptLineComponent({
  line,
  index,
  isDragging = false,
  dropPosition = null,
  onDragStart,
  onDragMove,
  onDragEnd,
}: ScriptLineProps) {
  const { t } = useTranslation();
  const { updateLine, assignCharacter, deleteLine, setGap, setInstructions } = useScriptStore();
  const { characters } = useCharacterStore();
  const currentProject = useProjectStore((s) => s.currentProject);
  const [generating, setGenerating] = useState(false);
  const [ttsError, setTtsError] = useState<string | null>(null);
  const [audioFragment, setAudioFragment] = useState<AudioFragment | null>(
    currentProject?.audio_fragments.find((a) => a.line_id === line.id) ?? null
  );

  useEffect(() => {
    if (!isDragging) return;
    const handler = (e: Event) => {
      e.preventDefault();
    };
    document.addEventListener('selectstart', handler);
    return () => {
      document.removeEventListener('selectstart', handler);
    };
  }, [isDragging]);

  const audioFragments = currentProject?.audio_fragments;

  useEffect(() => {
    const frag = audioFragments?.find((a) => a.line_id === line.id) ?? null;
    setAudioFragment(frag);
  }, [line.id, audioFragments]);

  useEffect(() => {
    if (audioFragment) setTtsError(null);
  }, [audioFragment]);

  const handleGenerateTts = async () => {
    if (!currentProject || !line.text.trim()) return;
    const character = characters.find((c) => c.id === line.character_id);
    setGenerating(true);
    setTtsError(null);
    try {
      const { saveScript } = useScriptStore.getState();
      await saveScript();

      const apiKey = await ipc.loadApiKey('dashscope');
      const fragment = await ipc.generateTts(
        currentProject.project.id,
        line.id,
        line.text,
        {
          tts_model: character?.tts_model ?? 'qwen3-tts-flash',
          voice_name: character?.voice_name ?? 'Cherry',
          speed: character?.speed ?? 1.0,
          pitch: character?.pitch ?? 1.0,
        },
        apiKey ?? '',
        line.instructions || undefined
      );
      setAudioFragment(fragment);
      const store = useProjectStore.getState();
      if (store.currentProject) {
        const existing = store.currentProject.audio_fragments.filter((a) => a.line_id !== fragment.line_id);
        useProjectStore.setState({
          currentProject: {
            ...store.currentProject,
            audio_fragments: [...existing, fragment],
          },
        });
      }
    } catch (e) {
      const msg = String(e);
      setTtsError(msg.length > 100 ? msg.slice(0, 100) + '...' : msg);
      useToastStore.getState().addToast(t('editor.ttsGenerateLineFailed', { line: String(index + 1) }));
    } finally {
      setGenerating(false);
    }
  };

  const handleRemoveAudio = useCallback(async () => {
    if (!audioFragment) return;
    try {
      await ipc.deleteAudioByLine(line.id);
      const store = useProjectStore.getState();
      if (store.currentProject) {
        const newFrags = store.currentProject.audio_fragments.filter((a) => a.line_id !== line.id);
        useProjectStore.setState({
          currentProject: {
            ...store.currentProject,
            audio_fragments: newFrags,
          },
        });
      }
      setAudioFragment(null);
    } catch {
      useToastStore.getState().addToast(t('editor.clearAudioFailed'));
    }
  }, [audioFragment, line.id, t]);

  const handleRecordingSave = useCallback((fragment: AudioFragment) => {
    setAudioFragment(fragment);
    const store = useProjectStore.getState();
    if (store.currentProject) {
      const existing = store.currentProject.audio_fragments.filter((a) => a.line_id !== fragment.line_id);
      useProjectStore.setState({
        currentProject: {
          ...store.currentProject,
          audio_fragments: [...existing, fragment],
        },
      });
    }
  }, []);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0 || !onDragStart) return;
      (e.target as Element).setPointerCapture(e.pointerId);
      onDragStart(line.id, e.pointerId);
    },
    [line.id, onDragStart]
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      onDragMove?.(e.clientX, e.clientY);
    },
    [onDragMove]
  );

  const handlePointerUp = useCallback(() => {
    onDragEnd?.();
  }, [onDragEnd]);

  // const _characterName = characters.find((c) => c.id === line.character_id)?.name;
  const UNASSIGNED = '__unassigned__';

  const insertIndicator = <div className="h-0.5 rounded-full bg-primary mx-1 transition-opacity" />;

  return (
    <div className="relative">
      {dropPosition === 'before' && insertIndicator}
      <div
        data-line-id={line.id}
        className={`group relative flex gap-0 rounded-lg border border-border/60 bg-card overflow-hidden transition-all duration-150 hover:border-border hover:shadow-sm ${isDragging ? 'opacity-40' : ''}`}
        style={isDragging ? { userSelect: 'none', WebkitUserSelect: 'none' } : undefined}
      >
        {/* Left gutter: drag handle + line number */}
        <div className="flex flex-col items-center gap-1 py-3 px-1.5 bg-muted/30 border-r border-border/40 shrink-0">
          <div
            className="cursor-grab text-muted-foreground/50 hover:text-muted-foreground active:cursor-grabbing touch-none transition-colors"
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
          >
            <GripVertical className="h-3.5 w-3.5" />
          </div>
          <span className="text-[10px] text-muted-foreground/60 font-mono tabular-nums">{index + 1}</span>
        </div>

        {/* Main content area */}
        <div className="flex-1 min-w-0 py-3 px-3 space-y-2">
          {/* Row 1: Character + text */}
          <div className="flex items-start gap-2">
            <div className="shrink-0 pt-0.5">
              <Select
                value={line.character_id ?? UNASSIGNED}
                onValueChange={(v) => {
                  assignCharacter(line.id, v === UNASSIGNED ? '' : v);
                }}
              >
                <SelectTrigger size="sm" className="h-6 text-xs min-w-[80px] max-w-[120px]">
                  <SelectValue placeholder={t('editor.unassigned')} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={UNASSIGNED}>{t('editor.unassigned')}</SelectItem>
                  {characters.map((c) => (
                    <SelectItem key={c.id} value={c.id}>
                      {c.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex-1 min-w-0">
              <textarea
                className="auto-grow-textarea"
                value={line.text}
                onChange={(e) => {
                  updateLine(line.id, e.target.value);
                }}
                placeholder={t('editor.linePlaceholder')}
              />
            </div>
          </div>

          {/* Row 2: Instructions (subtle, collapsible feel) */}
          <input
            type="text"
            className="w-full rounded-md border border-transparent bg-transparent px-2 py-1 text-xs text-muted-foreground placeholder:text-muted-foreground/50 hover:border-purple-200 hover:bg-purple-50/30 focus:border-purple-300 focus:bg-purple-50/30 dark:hover:border-purple-800 dark:hover:bg-purple-900/10 dark:focus:border-purple-700 dark:focus:bg-purple-900/10 focus-visible:ring-1 focus-visible:ring-purple-500/30 outline-none transition-all"
            value={line.instructions}
            onChange={(e) => {
              setInstructions(line.id, e.target.value);
            }}
            placeholder={t('editor.instructionsPlaceholder')}
          />

          {/* Row 3: Audio controls + metadata */}
          <div className="flex items-center gap-2 flex-wrap">
            {/* Audio status badge */}
            {ttsError && (
              <Badge variant="destructive" className="gap-1 text-[11px]" title={ttsError}>
                <AlertCircle className="h-3 w-3" /> {t('editor.generationFailed')}
                <button
                  className="ml-0.5 hover:opacity-80"
                  onClick={() => void handleGenerateTts()}
                  aria-label={t('editor.retry')}
                >
                  <RotateCcw className="h-3 w-3" />
                </button>
              </Badge>
            )}

            {audioFragment ? (
              <Badge
                variant="outline"
                className="text-green-600 dark:text-green-400 border-green-200 dark:border-green-800 gap-1 text-[11px]"
              >
                {audioFragment.source === 'recording' ? <Mic className="h-3 w-3" /> : <Volume2 className="h-3 w-3" />}
                {audioFragment.source === 'recording' ? t('editor.recorded') : t('editor.generated')}
                {audioFragment.duration_ms != null && (
                  <span className="opacity-60">({(audioFragment.duration_ms / 1000).toFixed(1)}s)</span>
                )}
              </Badge>
            ) : !ttsError ? (
              <Badge variant="outline" className="text-muted-foreground/60 border-border/60 text-[11px]">
                {t('editor.notGenerated')}
              </Badge>
            ) : null}

            {/* TTS generate button */}
            <Button
              size="xs"
              variant="outline"
              className="h-6 text-[11px] gap-1 border-blue-200 dark:border-blue-800 text-blue-600 dark:text-blue-400 hover:bg-blue-50 dark:hover:bg-blue-900/20"
              onClick={() => void handleGenerateTts()}
              disabled={generating || !line.text.trim()}
            >
              {generating ? <Loader2 className="h-3 w-3 animate-spin" /> : <Volume2 className="h-3 w-3" />}
              {generating ? t('editor.generatingTts') : t('editor.generateTts')}
            </Button>

            {/* Recorder */}
            <AudioRecorder
              lineId={line.id}
              onSave={handleRecordingSave}
              onRemove={() => void handleRemoveAudio()}
              hasExistingAudio={!!audioFragment}
            />

            {/* Gap control */}
            <span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground/70 ml-auto">
              {t('editor.gap')}
              <Input
                type="number"
                className="w-16 h-5 text-[11px] text-center px-1"
                value={line.gap_after_ms}
                onChange={(e) => {
                  setGap(line.id, parseInt(e.target.value) || 0);
                }}
                min={0}
                max={5000}
                step={100}
              />
              ms
            </span>
          </div>

          {/* Audio player (only when audio exists) */}
          {audioFragment && (
            <div className="pt-1">
              <AudioPlayer filePath={audioFragment.file_path} />
            </div>
          )}
        </div>

        {/* Right: delete button */}
        <div className="flex items-start pt-3 pr-2 shrink-0">
          <Button
            variant="ghost"
            size="icon-sm"
            className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity hover:text-destructive"
            onClick={() => {
              deleteLine(line.id);
            }}
            aria-label="Delete line"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
      {dropPosition === 'after' && insertIndicator}
    </div>
  );
}

export default memo(ScriptLineComponent);
