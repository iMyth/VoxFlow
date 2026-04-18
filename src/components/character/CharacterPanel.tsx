import { invoke } from '@tauri-apps/api/core';
import {
  Plus,
  Pencil,
  Trash2,
  Save,
  X,
  Download,
  Users,
  Mic,
  Upload,
  Play,
  Pause,
  Loader2,
  Sparkles,
} from 'lucide-react';
import { useState, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';

import * as ipc from '../../lib/ipc';
import { useCharacterStore } from '../../store/characterStore';
import { useProjectStore } from '../../store/projectStore';
import { useToastStore } from '../../store/toastStore';
import { Badge } from '../ui/badge';
import { Button } from '../ui/button';
import { Card, CardContent } from '../ui/card';
import { Input } from '../ui/input';
import { Label } from '../ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../ui/select';
import { Slider } from '../ui/slider';

import type { CharacterInput, Character } from '../../types';

// Voice cloning requires the VC (Voice Cloning) model
const VC_TTS_MODEL = 'qwen3-tts-vc-realtime-2026-01-15';

const defaultInput: CharacterInput = {
  name: '',
  tts_model: 'qwen3-tts-instruct-flash-realtime',
  voice_name: 'Cherry',
  speed: 1.0,
  pitch: 1.0,
};

export default function CharacterPanel() {
  const { t } = useTranslation();
  const { characters, createCharacter, updateCharacter, deleteCharacter } = useCharacterStore();
  const currentProject = useProjectStore((s) => s.currentProject);
  const [editing, setEditing] = useState<string | null>(null);
  const [form, setForm] = useState<CharacterInput>(defaultInput);
  const [isCreating, setIsCreating] = useState(false);

  // Import state
  const [showImport, setShowImport] = useState(false);
  const [importProjects, setImportProjects] = useState<[string, string, Character[]][]>([]);
  const [importSelected, setImportSelected] = useState<Set<string>>(new Set());

  // Voice cloning state
  const [vcMode, setVcMode] = useState<'standard' | 'cloning'>('standard');
  const [vcAudioBase64, setVcAudioBase64] = useState<string | null>(null);
  const [vcAudioDuration, setVcAudioDuration] = useState<number | null>(null);
  const [vcVoiceId, setVcVoiceId] = useState<string | null>(null);
  const [vcCreating, setVcCreating] = useState(false);
  const [vcPreviewing, setVcPreviewing] = useState(false);
  const [vcPreviewPath, setVcPreviewPath] = useState<string | null>(null);
  const [vcPlayingPreview, setVcPlayingPreview] = useState(false);
  const [vcRecording, setVcRecording] = useState(false);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const vcChunksRef = useRef<Blob[]>([]);
  const vcTimerRef = useRef<number | null>(null);
  const vcStartTimeRef = useRef<number>(0);
  const [vcRecordTime, setVcRecordTime] = useState(0);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const startCreate = () => {
    setIsCreating(true);
    setEditing(null);
    setForm(defaultInput);
    setVcMode('standard');
    setVcAudioBase64(null);
    setVcAudioDuration(null);
    setVcVoiceId(null);
    setVcPreviewPath(null);
    setVcPreviewing(false);
    setVcCreating(false);
  };

  const startEdit = (c: Character) => {
    setEditing(c.id);
    setIsCreating(false);
    setForm({
      name: c.name,
      tts_model: c.tts_model,
      voice_name: c.voice_name,
      speed: c.speed,
      pitch: c.pitch,
    });
    setVcMode('standard');
    setVcVoiceId(null);
    setVcPreviewPath(null);
  };

  const cancel = () => {
    setEditing(null);
    setIsCreating(false);
    setForm(defaultInput);
    setVcMode('standard');
    setVcAudioBase64(null);
    setVcAudioDuration(null);
    setVcVoiceId(null);
    setVcPreviewPath(null);
    setVcCreating(false);
  };

  const handleImportOpen = async () => {
    try {
      const all = await ipc.listAllProjectCharacters();
      const currentId = currentProject?.project.id ?? '';
      const filtered = all.filter(([pid, chars]) => pid !== currentId && chars.length > 0);
      setImportProjects(filtered);
      setImportSelected(new Set());
      setShowImport(true);
    } catch {
      useToastStore.getState().addToast('character.fetchFailed');
    }
  };

  const handleImportSubmit = async () => {
    if (importSelected.size === 0 || !currentProject) return;
    try {
      await ipc.importCharacters(currentProject.project.id, Array.from(importSelected));
      useToastStore.getState().addToast(t('character.importSuccess', { count: importSelected.size }), 'success');
      await useCharacterStore.getState().fetchCharacters();
    } catch {
      useToastStore.getState().addToast('character.importFailed');
    } finally {
      setShowImport(false);
    }
  };

  const toggleImport = (charId: string) => {
    setImportSelected((prev) => {
      const next = new Set(prev);
      if (next.has(charId)) next.delete(charId);
      else next.add(charId);
      return next;
    });
  };

  const handleSave = async () => {
    if (!form.name.trim()) return;

    // In cloning mode, the voice_name and tts_model are derived from the cloned
    // voice — not from the form fields (which are hidden in this mode).
    if (vcMode === 'cloning') {
      if (!vcVoiceId) {
        useToastStore.getState().addToast(t('character.vcVoiceRequired'));
        return;
      }
      const cloneForm = { ...form, voice_name: vcVoiceId, tts_model: VC_TTS_MODEL };
      if (isCreating) {
        await createCharacter(cloneForm);
      } else if (editing) {
        await updateCharacter(editing, cloneForm);
      }
      cancel();
      return;
    }

    if (isCreating) {
      await createCharacter(form);
    } else if (editing) {
      await updateCharacter(editing, form);
    }
    cancel();
  };

  const handleDelete = async (id: string) => {
    if (window.confirm(t('character.confirmDelete'))) {
      await deleteCharacter(id);
    }
  };

  // ---- Voice Cloning Functions ----

  const startVcRecording = useCallback(async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const recorder = new MediaRecorder(stream);
      vcChunksRef.current = [];

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) {
          vcChunksRef.current.push(e.data);
        }
      };

      recorder.onstop = () => {
        stream.getTracks().forEach((tr) => {
          tr.stop();
        });
        if (vcTimerRef.current) {
          clearInterval(vcTimerRef.current);
          vcTimerRef.current = null;
        }
        // Preserve recorder MIME type so the data URI has the correct format
        const mimeType = recorder.mimeType || 'audio/webm';
        const blob = new Blob(vcChunksRef.current, { type: mimeType });
        const reader = new FileReader();
        reader.onloadend = () => {
          // Store the full data URL (including MIME type prefix) so the backend
          // can use the correct Content-Type when sending to the API
          setVcAudioBase64(reader.result as string);
          setVcAudioDuration(Math.floor((Date.now() - vcStartTimeRef.current) / 1000));
        };
        reader.onerror = () => {
          useToastStore.getState().addToast(t('character.vcProcessingFailed'));
        };
        reader.readAsDataURL(blob);
      };

      recorder.start(100);
      mediaRecorderRef.current = recorder;
      setVcRecording(true);
      vcStartTimeRef.current = Date.now();
      setVcRecordTime(0);

      vcTimerRef.current = window.setInterval(() => {
        setVcRecordTime(Math.floor((Date.now() - vcStartTimeRef.current) / 1000));
      }, 500);
    } catch {
      useToastStore.getState().addToast(t('character.vcRecordingDenied'));
    }
  }, [t]);

  const stopVcRecording = useCallback(() => {
    if (!mediaRecorderRef.current || mediaRecorderRef.current.state === 'inactive') return;
    mediaRecorderRef.current.stop();
    setVcRecording(false);
  }, []);

  const handleVcFileUpload = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file) return;
      try {
        // Use FileReader for reliable base64 encoding
        const reader = new FileReader();
        reader.onloadend = () => {
          // Store the full data URL (including MIME type prefix) so the backend
          // receives the correct format information
          setVcAudioBase64(reader.result as string);
          // Estimate duration from file size (rough: ~12KB/s for speech audio)
          setVcAudioDuration(Math.max(1, Math.round(file.size / 12000)));
        };
        reader.onerror = () => {
          useToastStore.getState().addToast(t('character.vcFileUploadFailed'));
        };
        reader.readAsDataURL(file);
      } catch {
        useToastStore.getState().addToast(t('character.vcFileUploadFailed'));
      }
      // Reset file input
      e.target.value = '';
    },
    [t]
  );

  const handleVcCreateVoice = useCallback(async () => {
    if (!vcAudioBase64 || !currentProject) return;
    // Generate a valid preferred_name: alphanumeric only, max 20 chars
    const sanitizedName = (form.name || 'voice').replace(/[^a-zA-Z0-9_-]/g, '').slice(0, 20) || 'voice';

    // Debug logging
    console.log('[VoiceClone] Frontend - base64 length:', vcAudioBase64.length);
    console.log('[VoiceClone] Frontend - base64 preview:', vcAudioBase64.substring(0, 100) + '...');

    setVcCreating(true);
    try {
      const voiceId = await ipc.createVoice(currentProject.project.id, vcAudioBase64, sanitizedName, VC_TTS_MODEL);
      setVcVoiceId(voiceId);
      useToastStore.getState().addToast(t('character.vcCreated', { voice: voiceId.slice(0, 12) + '...' }), 'success');
    } catch (e) {
      useToastStore.getState().addToast(t('character.vcCreateFailed', { error: String(e) }));
    } finally {
      setVcCreating(false);
    }
  }, [vcAudioBase64, currentProject, form.name, t]);

  const handleVcPreview = useCallback(async () => {
    if (!vcVoiceId || !currentProject) return;
    setVcPreviewing(true);
    try {
      const path = await ipc.previewVoice(currentProject.project.id, vcVoiceId, VC_TTS_MODEL);
      setVcPreviewPath(path);
      setVcPreviewing(false);
      useToastStore.getState().addToast(t('character.vcPreviewReady'), 'success');
    } catch (e) {
      setVcPreviewing(false);
      useToastStore.getState().addToast(t('character.vcPreviewFailed', { error: String(e) }));
    }
  }, [vcVoiceId, currentProject, t]);

  const toggleVcPreviewPlay = useCallback(async () => {
    if (!vcPreviewPath) return;
    if (vcPlayingPreview) {
      await invoke('stop_audio');
      setVcPlayingPreview(false);
    } else {
      try {
        await invoke('play_audio', { filePath: vcPreviewPath });
        setVcPlayingPreview(true);
        setTimeout(() => {
          setVcPlayingPreview(false);
        }, 10000);
      } catch {
        useToastStore.getState().addToast(t('character.vcPlayFailed'));
      }
    }
  }, [vcPreviewPath, vcPlayingPreview, t]);

  const formatVcTime = (s: number) => {
    const m = Math.floor(s / 60);
    const sec = s % 60;
    return `${m.toString().padStart(2, '0')}:${sec.toString().padStart(2, '0')}`;
  };

  const renderForm = () => (
    <Card>
      <CardContent className="space-y-4">
        <div className="space-y-1.5">
          <Label>{t('character.name')}</Label>
          <Input
            value={form.name}
            onChange={(e) => {
              setForm({ ...form, name: e.target.value });
            }}
            placeholder={t('character.namePlaceholder')}
            autoFocus
          />
        </div>

        {/* Voice Mode Toggle */}
        <div className="flex gap-2">
          <Button
            size="sm"
            variant={vcMode === 'standard' ? 'default' : 'outline'}
            className="flex-1"
            onClick={() => {
              setVcMode('standard');
              setVcVoiceId(null);
              setVcPreviewPath(null);
            }}
          >
            {t('character.vcStandardMode')}
          </Button>
          <Button
            size="sm"
            variant={vcMode === 'cloning' ? 'default' : 'outline'}
            className="flex-1"
            onClick={() => {
              setVcMode('cloning');
            }}
          >
            <Sparkles className="h-3.5 w-3.5 mr-1" />
            {t('character.vcCloneMode')}
          </Button>
        </div>

        {vcMode === 'cloning' && (
          <div className="space-y-4 rounded-lg border border-purple-200 dark:border-purple-800 p-4 bg-purple-50/30 dark:bg-purple-900/10">
            <Label className="text-purple-700 dark:text-purple-300">{t('character.vcCloneMode')}</Label>

            {/* Audio Input */}
            {vcRecording ? (
              <div className="flex items-center gap-2">
                <span className="text-xs text-red-500 font-mono">
                  <span className="h-2 w-2 rounded-full bg-red-500 animate-pulse inline mr-1" />
                  {formatVcTime(vcRecordTime)}
                </span>
                <Button size="xs" variant="destructive" onClick={stopVcRecording}>
                  {t('character.vcStop')}
                </Button>
              </div>
            ) : !vcAudioBase64 ? (
              <div className="flex gap-2">
                <Button size="sm" variant="outline" onClick={startVcRecording}>
                  <Mic className="h-3.5 w-3.5 mr-1" />
                  {t('character.vcRecord')}
                </Button>
                <Button size="sm" variant="outline" onClick={() => fileInputRef.current?.click()}>
                  <Upload className="h-3.5 w-3.5 mr-1" />
                  {t('character.vcUpload')}
                </Button>
                <input
                  ref={fileInputRef}
                  type="file"
                  accept="audio/*"
                  className="hidden"
                  onChange={handleVcFileUpload}
                />
              </div>
            ) : (
              <div className="flex items-center gap-2">
                <Badge variant="outline" className="text-green-600 border-green-300">
                  {t('character.vcAudioReady', { duration: vcAudioDuration ?? 0 })}
                </Badge>
                <Button
                  size="xs"
                  variant="outline"
                  onClick={() => {
                    setVcAudioBase64(null);
                    setVcAudioDuration(null);
                  }}
                >
                  <X className="h-3 w-3" />
                </Button>
              </div>
            )}

            {/* Create Voice Button */}
            {vcAudioBase64 && !vcRecording && (
              <Button size="sm" onClick={() => void handleVcCreateVoice()} disabled={vcCreating}>
                {vcCreating ? (
                  <Loader2 className="h-3.5 w-3.5 mr-1 animate-spin" />
                ) : (
                  <Sparkles className="h-3.5 w-3.5 mr-1" />
                )}
                {vcCreating ? t('character.vcCreating') : t('character.vcCreateVoice')}
              </Button>
            )}

            {/* Voice ID & Preview */}
            {vcVoiceId && (
              <div className="space-y-2">
                <div className="flex items-center gap-1.5 flex-wrap">
                  <Badge variant="secondary" className="font-mono text-xs">
                    {vcVoiceId.slice(0, 16)}...
                  </Badge>
                  <span className="text-xs text-muted-foreground">{t('character.vcWillSave')}</span>
                </div>
                <div className="flex gap-2">
                  <Button size="xs" onClick={() => void handleVcPreview()} disabled={vcPreviewing}>
                    {vcPreviewing ? <Loader2 className="h-3 w-3 mr-1 animate-spin" /> : null}
                    {t('character.vcPreviewVoice')}
                  </Button>
                  {vcPreviewPath && (
                    <Button size="xs" variant="outline" onClick={() => void toggleVcPreviewPlay()}>
                      {vcPlayingPreview ? <Pause className="h-3 w-3" /> : <Play className="h-3 w-3" />}
                      {vcPlayingPreview ? t('editor.pause') : t('editor.play')}
                    </Button>
                  )}
                </div>
              </div>
            )}
          </div>
        )}

        {vcMode === 'standard' && (
          <>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-1.5">
                <Label>{t('character.ttsModel')}</Label>
                <Select
                  value={form.tts_model}
                  onValueChange={(v) => {
                    setForm({ ...form, tts_model: v });
                  }}
                  disabled={form.tts_model === VC_TTS_MODEL}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="qwen3-tts-flash">Qwen3 TTS Flash</SelectItem>
                    <SelectItem value="qwen3-tts-instruct-flash">Qwen3 TTS Instruct Flash</SelectItem>
                    <SelectItem value="qwen3-tts-instruct-flash-realtime">Qwen3 TTS Instruct Flash Realtime</SelectItem>
                    <SelectItem value={VC_TTS_MODEL}>Qwen3 TTS VC Realtime (Voice Cloning)</SelectItem>
                  </SelectContent>
                </Select>
                {form.tts_model === VC_TTS_MODEL && (
                  <p className="text-xs text-muted-foreground">{t('character.vcModelLocked')}</p>
                )}
              </div>
              <div className="space-y-1.5">
                <Label>{t('character.voice')}</Label>
                <Input
                  value={form.voice_name}
                  onChange={(e) => {
                    setForm({ ...form, voice_name: e.target.value });
                  }}
                  placeholder="Cherry"
                  disabled={form.tts_model === VC_TTS_MODEL}
                />
                {form.tts_model === VC_TTS_MODEL && (
                  <p className="text-xs text-muted-foreground">{t('character.vcVoiceLocked')}</p>
                )}
              </div>
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label>
                  {t('character.speed')} ({form.speed.toFixed(1)}x)
                </Label>
                <Slider
                  min={0.5}
                  max={2.0}
                  step={0.1}
                  value={[form.speed]}
                  onValueChange={([v]) => {
                    setForm({ ...form, speed: v });
                  }}
                />
              </div>
              <div className="space-y-2">
                <Label>
                  {t('character.pitch')} ({form.pitch.toFixed(1)}x)
                </Label>
                <Slider
                  min={0.5}
                  max={2.0}
                  step={0.1}
                  value={[form.pitch]}
                  onValueChange={([v]) => {
                    setForm({ ...form, pitch: v });
                  }}
                />
              </div>
            </div>
          </>
        )}

        <div className="flex gap-2 justify-end">
          <Button variant="outline" onClick={cancel}>
            <X className="h-4 w-4" /> {t('character.cancel')}
          </Button>
          <Button onClick={() => void handleSave()}>
            <Save className="h-4 w-4" /> {t('character.save')}
          </Button>
        </div>
      </CardContent>
    </Card>
  );

  return (
    <div className="px-6 py-8">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-xl font-bold">{t('character.title')}</h2>
        <div className="flex gap-2">
          <Button variant="outline" onClick={() => void handleImportOpen()}>
            <Download className="h-4 w-4 mr-1" /> {t('character.import')}
          </Button>
          <Button onClick={startCreate}>
            <Plus className="h-4 w-4" /> {t('character.create')}
          </Button>
        </div>
      </div>
      {isCreating && renderForm()}
      <div className="space-y-3 mt-4">
        {characters.map((c) =>
          editing === c.id ? (
            <div key={c.id}>{renderForm()}</div>
          ) : (
            <Card key={c.id}>
              <CardContent className="flex items-center justify-between">
                <div>
                  <p className="font-medium">{c.name}</p>
                  <p className="text-sm text-muted-foreground">
                    {c.tts_model} · {c.voice_name} · {t('character.speed')} {c.speed}x · {t('character.pitch')}{' '}
                    {c.pitch}x
                  </p>
                </div>
                <div className="flex gap-1">
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => {
                      startEdit(c);
                    }}
                    aria-label={`Edit ${c.name}`}
                  >
                    <Pencil className="h-4 w-4" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => void handleDelete(c.id)}
                    aria-label={`Delete ${c.name}`}
                    className="hover:text-destructive"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </CardContent>
            </Card>
          )
        )}
        {characters.length === 0 && !isCreating && (
          <p className="text-center text-muted-foreground py-12">{t('character.empty')}</p>
        )}
      </div>

      {/* Import dialog */}
      {showImport && (
        <div className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center">
          <div className="bg-background rounded-xl border shadow-xl max-w-lg w-full mx-4 max-h-[80vh] flex flex-col">
            <div className="flex items-center justify-between px-6 py-4 border-b">
              <h3 className="text-lg font-semibold">{t('character.import')}</h3>
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => {
                  setShowImport(false);
                }}
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
            <div className="flex-1 overflow-auto px-6 py-4">
              {importProjects.length === 0 ? (
                <p className="text-center text-muted-foreground py-8">{t('character.importEmpty')}</p>
              ) : (
                <div className="space-y-4">
                  {importProjects.map(([projectId, projectName, chars]) => (
                    <div key={projectId}>
                      <p className="text-sm font-medium mb-2 text-foreground">
                        <Users className="h-3 w-3 inline mr-1" />
                        {projectName}
                      </p>
                      <div className="space-y-1">
                        {chars.map((c) => (
                          <div
                            key={c.id}
                            className={`flex items-center gap-2 rounded-lg border px-3 py-2 cursor-pointer transition ${
                              importSelected.has(c.id)
                                ? 'border-primary bg-primary/5'
                                : 'border-border hover:bg-accent/50'
                            }`}
                            onClick={() => {
                              toggleImport(c.id);
                            }}
                          >
                            <Badge variant="secondary" className="text-xs">
                              {c.name}
                            </Badge>
                            <span className="text-xs text-muted-foreground">
                              {c.voice_name} ({c.tts_model})
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
            <div className="flex items-center justify-end gap-2 px-6 py-4 border-t">
              <Button
                variant="outline"
                onClick={() => {
                  setShowImport(false);
                }}
              >
                {t('character.cancel')}
              </Button>
              <Button onClick={() => void handleImportSubmit()} disabled={importSelected.size === 0}>
                {t('character.importSelected', { count: importSelected.size })}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
