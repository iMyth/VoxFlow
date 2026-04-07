import { create } from 'zustand';
import * as ipc from '../lib/ipc';
import { useToastStore } from './toastStore';
import type { UserSettings } from '../types';

interface SettingsStore {
    llmEndpoint: string;
    llmModel: string;
    defaultTtsModel: string;
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
    llmEndpoint: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    llmModel: 'qwen3.5-plus',
    defaultTtsModel: 'qwen3-tts-flash',
    defaultVoiceName: 'Cherry',
    defaultSpeed: 1.0,
    defaultPitch: 1.0,

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
            });
        } catch (e) {
            useToastStore.getState().addToast('加载设置失败');
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
        };
        try {
            await ipc.saveSettings(settings);
        } catch (e) {
            useToastStore.getState().addToast('保存设置失败');
        }
    },

    saveApiKey: async (service: string, key: string) => {
        try {
            await ipc.saveApiKey(service, key);
        } catch (e) {
            useToastStore.getState().addToast('保存 API Key 失败');
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
