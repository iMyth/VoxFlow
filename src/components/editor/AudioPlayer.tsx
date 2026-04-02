import { useRef, useState } from 'react';
import { Play, Pause } from 'lucide-react';
import { convertFileSrc } from '@tauri-apps/api/core';

interface AudioPlayerProps {
    filePath: string;
}

export default function AudioPlayer({ filePath }: AudioPlayerProps) {
    const audioRef = useRef<HTMLAudioElement>(null);
    const [playing, setPlaying] = useState(false);

    const src = convertFileSrc(filePath);

    const toggle = () => {
        if (!audioRef.current) return;
        if (playing) {
            audioRef.current.pause();
        } else {
            audioRef.current.play();
        }
        setPlaying(!playing);
    };

    return (
        <span className="inline-flex items-center">
            <audio
                ref={audioRef}
                src={src}
                onEnded={() => setPlaying(false)}
                preload="none"
            />
            <button
                className="flex items-center gap-1 rounded-lg border border-gray-300 dark:border-gray-600 px-2 py-1 text-xs hover:bg-gray-100 dark:hover:bg-gray-700"
                onClick={toggle}
                aria-label={playing ? 'Pause' : 'Play'}
            >
                {playing ? <Pause className="h-3 w-3" /> : <Play className="h-3 w-3" />}
                {playing ? '暂停' : '播放'}
            </button>
        </span>
    );
}
