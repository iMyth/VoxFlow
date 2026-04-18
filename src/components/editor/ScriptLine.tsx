import { GripVertical, Trash2, Volume2, Loader2, AlertCircle, Mic } from 'lucide-react';
import { useState, useEffect, useCallback } from 'react';
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
import { Card } from '../ui/card';
import { Input } from '../ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';

import type { ScriptLine, AudioFragment } from '../../types';

/* ---- no module-level drag state, all state lifted to parent ---- */

interface ScriptLineProps {
  line: ScriptLine;
  index: number;
  totalLines?: number;
  isDragging?: boolean;
  /** Where the insertion indicator should appear: 'before' or 'after' this card */
  dropPosition?: 'before' | 'after' | null;
  /** Called when pointer is down on the grip (starts drag) */
  onDragStart?: (lineId: string, pointerId: number) => void;
  /** Called when pointer moves during drag (to find drop target) */
  onDragMove?: (clientX: number, clientY: number) => void;
  /** Called when pointer is released (completes drag) */
  onDragEnd?: () => void;
}

export default function ScriptLineComponent({
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

  // Prevent text selection globally while any card is being dragged
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

  // Sync local audioFragment with project store (e.g. after batch TTS)
  // Use a string of IDs as dependency to avoid re-rendering on array reference changes
  const audioFragmentIdStr = currentProject?.audio_fragments.map((a) => `${a.line_id}:${a.file_path}`).join('|');

  useEffect(() => {
    const frag = currentProject?.audio_fragments.find((a) => a.line_id === line.id) ?? null;
    setAudioFragment(frag);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [line.id, audioFragmentIdStr]);

  // Clear error when audio fragment appears
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

  const handleRemoveAudio = async () => {
    if (!audioFragment) return;
    try {
      await ipc.clearAudioFragments(audioFragment.project_id);
      // Re-fetch project to sync audio fragments
      const store = useProjectStore.getState();
      if (store.currentProject && store.currentProject.project.id === audioFragment.project_id) {
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
  };

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

  /* ---- Pointer drag on grip handle ---- */
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

  const characterName = characters.find((c) => c.id === line.character_id)?.name;
  const UNASSIGNED = '__unassigned__';

  const insertIndicator = <div className="h-0.5 rounded-full bg-primary mx-1 transition-opacity" />;

  return (
    <div className="relative">
      {dropPosition === 'before' && insertIndicator}
      <Card
        data-line-id={line.id}
        className={`flex-row items-start gap-2 p-3 group transition-all duration-150 ${isDragging ? 'opacity-40' : ''}`}
        style={isDragging ? { userSelect: 'none', WebkitUserSelect: 'none' } : undefined}
      >
        {/* Drag handle: only this area initiates drag, text area remains fully selectable */}
        <div
          className="cursor-grab select-none pt-2 text-muted-foreground hover:text-foreground active:cursor-grabbing touch-none"
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
        >
          <GripVertical className="h-4 w-4" />
        </div>

        <span className="pt-2 text-xs text-muted-foreground w-6 text-right shrink-0">{index + 1}</span>

        <div className="flex-1 space-y-2">
          {/* Auto-growing textarea using CSS field-sizing: content */}
          <textarea
            className="auto-grow-textarea"
            value={line.text}
            onChange={(e) => {
              updateLine(line.id, e.target.value);
            }}
            placeholder={t('editor.linePlaceholder')}
          />
          <input
            type="text"
            className="w-full rounded-md border border-purple-300/50 bg-purple-50/30 dark:bg-purple-900/10 px-3 py-1.5 text-xs text-purple-700 dark:text-purple-300 placeholder:text-purple-400/80 dark:placeholder:text-purple-300/70 focus-visible:border-purple-500 focus-visible:ring-2 focus-visible:ring-purple-500/30 outline-none"
            value={line.instructions}
            onChange={(e) => {
              setInstructions(line.id, e.target.value);
            }}
            placeholder={t('editor.instructionsPlaceholder')}
          />
          <div className="flex items-center gap-3 flex-wrap">
            <Select
              value={line.character_id ?? UNASSIGNED}
              onValueChange={(v) => {
                assignCharacter(line.id, v === UNASSIGNED ? '' : v);
              }}
            >
              <SelectTrigger size="sm">
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

            {characterName && <Badge variant="secondary">{characterName}</Badge>}

            {ttsError && (
              <Badge variant="destructive" className="gap-1">
                <AlertCircle className="h-3 w-3" /> {t('editor.generationFailed')}
              </Badge>
            )}

            {audioFragment ? (
              <Badge variant="outline" className="text-green-600 border-green-300 gap-1">
                {audioFragment.source === 'recording' ? (
                  <>
                    <Mic className="h-3 w-3" /> {t('editor.recorded')}
                  </>
                ) : (
                  <>
                    <Volume2 className="h-3 w-3" /> {t('editor.generated')}
                  </>
                )}
                {audioFragment.duration_ms != null && (
                  <span className="text-xs opacity-70">({(audioFragment.duration_ms / 1000).toFixed(1)}s)</span>
                )}
              </Badge>
            ) : !ttsError ? (
              <Badge variant="outline" className="text-muted-foreground">
                {t('editor.notGenerated')}
              </Badge>
            ) : null}

            <Button size="xs" onClick={() => void handleGenerateTts()} disabled={generating || !line.text.trim()}>
              {generating ? <Loader2 className="h-3 w-3 animate-spin" /> : <Volume2 className="h-3 w-3" />}
              {generating ? t('editor.generatingTts') : t('editor.generateTts')}
            </Button>

            <AudioRecorder
              lineId={line.id}
              onSave={handleRecordingSave}
              onRemove={handleRemoveAudio}
              hasExistingAudio={!!audioFragment}
            />

            {audioFragment && <AudioPlayer filePath={audioFragment.file_path} />}

            <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
              {t('editor.gap')}
              <Input
                type="number"
                className="w-24 h-6 text-xs text-center"
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
        </div>

        <Button
          variant="ghost"
          size="icon-sm"
          className="pt-2 opacity-0 group-hover:opacity-100 transition hover:text-destructive"
          onClick={() => {
            deleteLine(line.id);
          }}
          aria-label="Delete line"
        >
          <Trash2 className="h-4 w-4" />
        </Button>
      </Card>
      {dropPosition === 'after' && insertIndicator}
    </div>
  );
}
