import { create } from 'zustand';
import * as ipc from '../lib/ipc';
import { useToastStore } from './toastStore';
import type { Character, CharacterInput } from '../types';

interface CharacterStore {
    characters: Character[];
    fetchCharacters: () => Promise<void>;
    createCharacter: (input: CharacterInput) => Promise<void>;
    updateCharacter: (id: string, input: CharacterInput) => Promise<void>;
    deleteCharacter: (id: string) => Promise<void>;
}

export const useCharacterStore = create<CharacterStore>((set) => ({
    characters: [],

    fetchCharacters: async () => {
        const { useProjectStore } = await import('./projectStore');
        const projectId = useProjectStore.getState().currentProject?.project.id;
        if (!projectId) return;
        try {
            const characters = await ipc.listCharacters(projectId);
            set({ characters });
        } catch (e) {
            useToastStore.getState().addToast('获取角色列表失败');
        }
    },

    createCharacter: async (input: CharacterInput) => {
        const { useProjectStore } = await import('./projectStore');
        const projectId = useProjectStore.getState().currentProject?.project.id;
        if (!projectId) return;
        try {
            const character = await ipc.createCharacter(projectId, input);
            set((state) => ({ characters: [...state.characters, character] }));
        } catch (e) {
            useToastStore.getState().addToast('创建角色失败');
        }
    },

    updateCharacter: async (id: string, input: CharacterInput) => {
        try {
            const updated = await ipc.updateCharacter(id, input);
            set((state) => ({
                characters: state.characters.map((c) => (c.id === id ? updated : c)),
            }));
        } catch (e) {
            useToastStore.getState().addToast('更新角色失败');
        }
    },

    deleteCharacter: async (id: string) => {
        try {
            await ipc.deleteCharacter(id);
            set((state) => ({
                characters: state.characters.filter((c) => c.id !== id),
            }));
        } catch (e) {
            useToastStore.getState().addToast('删除角色失败');
        }
    },
}));
