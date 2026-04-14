import { create } from 'zustand';

import { useToastStore } from './toastStore';
import * as ipc from '../lib/ipc';

import type { UserSettings } from '../types';

interface SettingsStore {
  llmEndpoint: string;
  llmModel: string;
  defaultTtsModel: string;
  defaultVoiceName: string;
  defaultSpeed: number;
  defaultPitch: number;
  enableThinking: boolean;
  loadSettings: () => Promise<void>;
  saveSettings: () => Promise<void>;
  saveApiKey: (service: string, key: string) => Promise<void>;
  loadApiKey: (service: string) => Promise<string | null>;
  set: (
    partial: Partial<Omit<SettingsStore, 'loadSettings' | 'saveSettings' | 'saveApiKey' | 'loadApiKey' | 'set'>>
  ) => void;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  llmEndpoint: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
  llmModel: 'qwen3.6-plus',
  defaultTtsModel: 'qwen3-tts-flash',
  defaultVoiceName: 'Cherry',
  defaultSpeed: 1.0,
  defaultPitch: 1.0,
  enableThinking: true,

  loadSettings: async () => {
    try {
      const settings = await ipc.loadSettings();
      set({
        llmEndpoint: settings.llm_endpoint,
        llmModel: settings.llm_model,
        defaultTtsModel: settings.default_tts_model,
        defaultVoiceName: settings.default_voice_name,
        defaultSpeed: settings.default_speed,
        defaultPitch: settings.default_pitch,
        enableThinking: settings.enable_thinking,
      });
    } catch {
      useToastStore.getState().addToast('settings.loadFailed');
    }
  },

  saveSettings: async () => {
    const state = get();
    const settings: UserSettings = {
      llm_endpoint: state.llmEndpoint,
      llm_model: state.llmModel,
      default_tts_model: state.defaultTtsModel,
      default_voice_name: state.defaultVoiceName,
      default_speed: state.defaultSpeed,
      default_pitch: state.defaultPitch,
      enable_thinking: state.enableThinking,
    };
    try {
      await ipc.saveSettings(settings);
    } catch {
      useToastStore.getState().addToast('settings.saveFailed');
    }
  },

  saveApiKey: async (service: string, key: string) => {
    try {
      await ipc.saveApiKey(service, key);
    } catch {
      useToastStore.getState().addToast('settings.saveApiKeyFailed');
    }
  },

  loadApiKey: async (service: string) => {
    try {
      return await ipc.loadApiKey(service);
    } catch {
      useToastStore.getState().addToast('settings.loadApiKeyFailed');
      return null;
    }
  },

  set: (partial) => {
    set(partial);
  },
}));
