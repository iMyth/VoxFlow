import { create } from 'zustand';

import type { SectionStyleConfig, SectionStatus } from '../types';

interface SectionVideoState {
  configs: Record<string, SectionStyleConfig>;
  statuses: Record<string, SectionStatus>;
  audioReady: Record<string, boolean>;
  videoReady: Record<string, boolean>;
  batchInProgress: boolean;
  batchCompleted: number;
  batchFailed: number;
  batchTotal: number;
  transitionDurationMs: number;

  setConfig: (sectionId: string, config: SectionStyleConfig) => void;
  setStatus: (sectionId: string, status: SectionStatus) => void;
  setAudioReady: (sectionId: string, ready: boolean) => void;
  setVideoReady: (sectionId: string, ready: boolean) => void;
  setBatchState: (
    partial: Partial<Pick<SectionVideoState, 'batchInProgress' | 'batchCompleted' | 'batchFailed' | 'batchTotal'>>
  ) => void;
  resetAll: () => void;
}

const initialState = {
  configs: {} as Record<string, SectionStyleConfig>,
  statuses: {} as Record<string, SectionStatus>,
  audioReady: {} as Record<string, boolean>,
  videoReady: {} as Record<string, boolean>,
  batchInProgress: false,
  batchCompleted: 0,
  batchFailed: 0,
  batchTotal: 0,
  transitionDurationMs: 500,
};

export const useSectionVideoStore = create<SectionVideoState>((set) => ({
  ...initialState,

  setConfig: (sectionId, config) => {
    set((state) => ({
      configs: { ...state.configs, [sectionId]: config },
    }));
  },

  setStatus: (sectionId, status) => {
    set((state) => ({
      statuses: { ...state.statuses, [sectionId]: status },
    }));
  },

  setAudioReady: (sectionId, ready) => {
    set((state) => ({
      audioReady: { ...state.audioReady, [sectionId]: ready },
    }));
  },

  setVideoReady: (sectionId, ready) => {
    set((state) => ({
      videoReady: { ...state.videoReady, [sectionId]: ready },
    }));
  },

  setBatchState: (partial) => {
    set(partial);
  },

  resetAll: () => {
    set(initialState);
  },
}));
