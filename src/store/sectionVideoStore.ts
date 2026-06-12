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
  loadProjectConfigs: (projectId: string) => void;
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

// Storage key prefix for localStorage
const STORAGE_KEY_PREFIX = 'voxflow-section-video-';

/**
 * Load section video configs from localStorage for a specific project
 */
function loadFromStorage(projectId: string): Partial<typeof initialState> {
  try {
    const key = `${STORAGE_KEY_PREFIX}${projectId}`;
    const stored = localStorage.getItem(key);
    if (stored) {
      return JSON.parse(stored) as Partial<typeof initialState>;
    }
  } catch (e) {
    console.error('Failed to load section video configs from localStorage:', e);
  }
  return {};
}

/**
 * Save section video configs to localStorage for a specific project
 */
function saveToStorage(projectId: string, configs: Record<string, SectionStyleConfig>, transitionDurationMs: number) {
  try {
    const key = `${STORAGE_KEY_PREFIX}${projectId}`;
    const data = { configs, transitionDurationMs };
    localStorage.setItem(key, JSON.stringify(data));
  } catch (e) {
    console.error('Failed to save section video configs to localStorage:', e);
  }
}

export const useSectionVideoStore = create<SectionVideoState>((set) => ({
  ...initialState,

  setConfig: (sectionId, config) => {
    set((state) => {
      const newConfigs = { ...state.configs, [sectionId]: config };

      // Auto-save to localStorage if we have a project context
      // We'll use the sectionId to infer the project (this is a simplification)
      // In practice, the App component will call loadProjectConfigs when switching projects
      const projectId = getProjectIdFromSectionId(sectionId);
      if (projectId) {
        saveToStorage(projectId, newConfigs, state.transitionDurationMs);
      }

      return { configs: newConfigs };
    });
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

  loadProjectConfigs: (projectId) => {
    const stored = loadFromStorage(projectId);
    set({
      ...initialState, // Reset to initial state first
      ...stored,
      // Ensure we have default values for missing fields
      configs: stored.configs || {},
      transitionDurationMs: stored.transitionDurationMs || 500,
    });
  },

  resetAll: () => {
    set(initialState);
  },
}));

/**
 * Extract project ID from section ID
 * This is a temporary solution - ideally we'd pass projectId explicitly
 * For now, we'll use a global variable or context to track the current project
 */
let currentProjectId: string | null = null;

export const setCurrentProjectId = (projectId: string | null) => {
  currentProjectId = projectId;
};

const getProjectIdFromSectionId = (_sectionId: string): string | null => {
  // Return the current project ID
  return currentProjectId;
};
