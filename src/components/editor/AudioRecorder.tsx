import { Mic, Square, Loader2, Trash2 } from 'lucide-react';
import { useState, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';

import * as ipc from '../../lib/ipc';
import { useProjectStore } from '../../store/projectStore';
import { useToastStore } from '../../store/toastStore';
import { Button } from '../ui/button';

import type { AudioFragment } from '../../types';

interface AudioRecorderProps {
  lineId: string;
  /** Called when recording is saved successfully with the new AudioFragment */
  onSave: (fragment: AudioFragment) => void;
  /** Remove the current audio fragment for this line */
  onRemove: () => void;
  /** Whether there's already an audio fragment for this line */
  hasExistingAudio: boolean;
}

export default function AudioRecorder({ lineId, onSave, onRemove, hasExistingAudio }: AudioRecorderProps) {
  const { t } = useTranslation();
  const [isRecording, setIsRecording] = useState(false);
  const [recordingTime, setRecordingTime] = useState(0);
  const [isSaving, setIsSaving] = useState(false);
  const [hasUnsavedRecording, setHasUnsavedRecording] = useState(false);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const timerRef = useRef<number | null>(null);
  const startTimeRef = useRef<number>(0);

  const startRecording = useCallback(async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const recorder = new MediaRecorder(stream);
      chunksRef.current = [];
      setHasUnsavedRecording(false);

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) {
          chunksRef.current.push(e.data);
        }
      };

      recorder.onstop = () => {
        stream.getTracks().forEach((t) => {
          t.stop();
        });
        if (timerRef.current) {
          clearInterval(timerRef.current);
          timerRef.current = null;
        }
      };

      // Use timeslice (100ms) to ensure data is collected regularly
      recorder.start(100);
      mediaRecorderRef.current = recorder;
      setIsRecording(true);
      startTimeRef.current = Date.now();
      setRecordingTime(0);

      timerRef.current = window.setInterval(() => {
        setRecordingTime(Math.floor((Date.now() - startTimeRef.current) / 1000));
      }, 500);
    } catch {
      useToastStore.getState().addToast(t('editor.recordingPermissionDenied'));
    }
  }, [t]);

  const stopRecording = useCallback(() => {
    if (!mediaRecorderRef.current || mediaRecorderRef.current.state === 'inactive') {
      return;
    }
    mediaRecorderRef.current.requestData();
    mediaRecorderRef.current.stop();
    // Wait for ondataavailable + onstop to fire before updating UI state
    setTimeout(() => {
      setHasUnsavedRecording(chunksRef.current.length > 0);
      setIsRecording(false);
    }, 10);
  }, []);

  const saveRecording = useCallback(async () => {
    if (chunksRef.current.length === 0) {
      return;
    }

    setIsSaving(true);
    try {
      const blob = new Blob(chunksRef.current, { type: 'audio/webm' });
      const arrayBuffer = await blob.arrayBuffer();
      // Chunked base64 encoding without padding artifacts
      const uint8 = new Uint8Array(arrayBuffer);
      let base64 = '';
      const chunkSize = 8192;
      for (let i = 0; i < uint8.length; i += chunkSize) {
        const chunk = uint8.subarray(i, i + chunkSize);
        base64 += String.fromCharCode(...chunk);
      }
      // Encode the full binary string as base64
      base64 = btoa(base64);

      const currentProject = useProjectStore.getState().currentProject;
      if (!currentProject) {
        useToastStore.getState().addToast(t('editor.noProjectSelected'));
        return;
      }

      const fragment = await ipc.importAudio(currentProject.project.id, lineId, base64);
      onSave(fragment);
      useToastStore.getState().addToast(t('editor.recordingSaved'));
    } catch (e) {
      console.error('[AudioRecorder] saveRecording failed:', e);
      useToastStore.getState().addToast(t('editor.recordingSaveFailed'));
    } finally {
      setIsSaving(false);
      setHasUnsavedRecording(false);
      chunksRef.current = [];
    }
  }, [lineId, onSave, t]);

  const discardRecording = useCallback(() => {
    setHasUnsavedRecording(false);
    chunksRef.current = [];
    setRecordingTime(0);
  }, []);

  const formatTime = (seconds: number) => {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
  };

  // If not recording and no unsaved recording, show just the mic button
  if (!isRecording && !hasUnsavedRecording) {
    return (
      <div className="inline-flex items-center gap-1">
        <Button
          size="xs"
          variant="outline"
          className="text-red-500 border-red-300 dark:border-red-700 hover:bg-red-50 dark:hover:bg-red-900/20"
          onClick={startRecording}
          disabled={isSaving}
        >
          <Mic className="h-3 w-3" />
          {t('editor.record')}
        </Button>
        {hasExistingAudio && (
          <Button
            size="xs"
            variant="ghost"
            className="h-5 w-5 p-0 text-muted-foreground hover:text-destructive"
            onClick={onRemove}
            title={t('editor.removeAudio')}
          >
            <Trash2 className="h-3 w-3" />
          </Button>
        )}
      </div>
    );
  }

  // Recording or has unsaved recording — show controls
  return (
    <div className="inline-flex items-center gap-2">
      {isRecording ? (
        <>
          <span className="inline-flex items-center gap-1.5 text-xs text-red-500 font-mono">
            <span className="h-2 w-2 rounded-full bg-red-500 animate-pulse" />
            {formatTime(recordingTime)}
          </span>
          <Button size="xs" variant="destructive" onClick={stopRecording}>
            <Square className="h-3 w-3" />
            {t('editor.stopRecording')}
          </Button>
        </>
      ) : (
        <>
          <span className="text-xs text-muted-foreground font-mono">{formatTime(recordingTime)}</span>
          <Button size="xs" onClick={saveRecording} disabled={isSaving}>
            {isSaving ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
            {t('editor.saveRecording')}
          </Button>
          <Button size="xs" variant="ghost" onClick={discardRecording} disabled={isSaving}>
            {t('editor.discardRecording')}
          </Button>
        </>
      )}
    </div>
  );
}
