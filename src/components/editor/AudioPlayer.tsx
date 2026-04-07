import { useEffect } from 'react';
import { Play, Pause } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useToastStore } from '../../store/toastStore';
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

    const clearIfPlaying = useAudioStore((s) => s.clearIfPlaying);

    useEffect(() => {
        const unlisten = listen('audio-finished', (e) => {
            const finishedPath = e.payload as string;
            clearIfPlaying(finishedPath);
        });
        return () => { unlisten.then((fn) => fn()); };
    }, [clearIfPlaying]);

    const toggle = async () => {
        try {
            if (isPlaying) {
                await invoke('stop_audio');
                setPlayingPath(null);
            } else {
                await invoke('play_audio', { filePath });
                setPlayingPath(filePath);
            }
        } catch (e) {
            useToastStore.getState().addToast('editor.audioPlaybackFailed');
            setPlayingPath(null);
        }
    };

    return (
        <Button
            variant="outline"
            size="xs"
            onClick={toggle}
            aria-label={isPlaying ? t('editor.pause') : t('editor.play')}
        >
            {isPlaying ? <Pause className="h-3 w-3" /> : <Play className="h-3 w-3" />}
            {isPlaying ? t('editor.pause') : t('editor.play')}
        </Button>
    );
}
