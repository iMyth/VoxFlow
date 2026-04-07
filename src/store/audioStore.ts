import { create } from 'zustand';

interface AudioState {
    playingPath: string | null;
    setPlayingPath: (path: string | null) => void;
    clearIfPlaying: (path: string) => void;
}

export const useAudioStore = create<AudioState>((set) => ({
    playingPath: null,
    setPlayingPath: (path) => set({ playingPath: path }),
    clearIfPlaying: (path: string) => {
        set((state) => {
            if (state.playingPath === path) {
                return { playingPath: null };
            }
            return {};
        });
    },
}));
