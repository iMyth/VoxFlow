import { create } from 'zustand';
import * as ipc from '../lib/ipc';
import type { UserSettings } from '../types';

interface SettingsStore {
    llmEndpoint: string;
    llmModel: string;
    defaultTtsEngine: 'edge-tts' | 'azure-tts' | 'dashscope-tts';
    defaultVoiceName: string;
    defaultSpeed: number;
    defaultPitch: number;
    loadSettings: () => Promise<void>;
    saveSettings: () => Promise<void>;
    saveApiKey: (service: string, key: string) => Promise<void>;
    loadApiKey: (service: string) => Promise<string | null>;
    set: (partial: Partial<Omit<SettingsStore, 'loadSettings' | 'saveSettings' | 'saveApiKey' | 'loadApiKey' | 'set'>>) => void;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
    llmEndpoint: 'https://api.openai.com/v1',
    llmModel: 'gpt-4',
    defaultTtsEngine: 'edge-tts',
    defaultVoiceName: 'zh-CN-XiaoxiaoNeural',
    defaultSpeed: 1.0,
    defaultPitch: 1.0,

    loadSettings: async () => {
        try {
            const settings = await ipc.loadSettings();
            set({
                llmEndpoint: settings.llm_endpoint,
                llmModel: settings.llm_model,
                defaultTtsEngine: settings.default_tts_engine,
                defaultVoiceName: settings.default_voice_name,
                defaultSpeed: settings.default_speed,
                defaultPitch: settings.default_pitch,
            });
        } catch (e) {
            console.error('Failed to load settings:', e);
        }
    },

    saveSettings: async () => {
        const state = get();
        const settings: UserSettings = {
            llm_endpoint: state.llmEndpoint,
            llm_model: state.llmModel,
            default_tts_engine: state.defaultTtsEngine,
            default_voice_name: state.defaultVoiceName,
            default_speed: state.defaultSpeed,
            default_pitch: state.defaultPitch,
        };
        try {
            await ipc.saveSettings(settings);
        } catch (e) {
            console.error('Failed to save settings:', e);
        }
    },

    saveApiKey: async (service: string, key: string) => {
        try {
            await ipc.saveApiKey(service, key);
        } catch (e) {
            console.error('Failed to save API key:', e);
        }
    },

    loadApiKey: async (service: string) => {
        try {
            return await ipc.loadApiKey(service);
        } catch (e) {
            console.error('Failed to load API key:', e);
            return null;
        }
    },

    set: (partial) => set(partial),
}));
