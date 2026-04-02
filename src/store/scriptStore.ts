import { create } from 'zustand';
import * as ipc from '../lib/ipc';
import type { ScriptLine } from '../types';

interface ScriptStore {
    lines: ScriptLine[];
    isGenerating: boolean;
    isDirty: boolean;
    streamingText: string;
    generateScript: (outline: string) => Promise<void>;
    updateLine: (lineId: string, text: string) => void;
    assignCharacter: (lineId: string, characterId: string) => void;
    addLine: (afterIndex: number) => void;
    deleteLine: (lineId: string) => void;
    reorderLines: (fromIndex: number, toIndex: number) => void;
    saveScript: () => Promise<void>;
}

export const useScriptStore = create<ScriptStore>((set, get) => ({
    lines: [],
    isGenerating: false,
    isDirty: false,
    streamingText: '',

    generateScript: async (outline: string) => {
        const { useProjectStore } = await import('./projectStore');
        const { useSettingsStore } = await import('./settingsStore');
        const project = useProjectStore.getState().currentProject;
        if (!project) return;

        const settings = useSettingsStore.getState();
        const apiKey = await ipc.loadApiKey('llm');

        set({ isGenerating: true, streamingText: '', lines: [] });

        const unlistenToken = await ipc.onLlmToken((token) => {
            set((state) => ({ streamingText: state.streamingText + token }));
        });

        const unlistenComplete = await ipc.onLlmComplete(() => {
            // Reload script lines from backend after generation completes
            ipc.loadScript(project.project.id).then((lines) => {
                set({ lines, isGenerating: false, streamingText: '', isDirty: false });
            });
        });

        const unlistenError = await ipc.onLlmError((error) => {
            console.error('LLM generation error:', error);
            set({ isGenerating: false });
        });

        try {
            await ipc.generateScript(project.project.id, outline, {
                api_endpoint: settings.llmEndpoint,
                api_key: apiKey ?? '',
                model: settings.llmModel,
            });
        } catch (e) {
            console.error('Failed to generate script:', e);
            set({ isGenerating: false });
        } finally {
            unlistenToken();
            unlistenComplete();
            unlistenError();
        }
    },

    updateLine: (lineId: string, text: string) => {
        set((state) => ({
            lines: state.lines.map((l) => (l.id === lineId ? { ...l, text } : l)),
            isDirty: true,
        }));
    },

    assignCharacter: (lineId: string, characterId: string) => {
        set((state) => ({
            lines: state.lines.map((l) =>
                l.id === lineId ? { ...l, character_id: characterId } : l,
            ),
            isDirty: true,
        }));
    },

    addLine: (afterIndex: number) => {
        set((state) => {
            const projectId = state.lines[0]?.project_id ?? '';
            const newLine: ScriptLine = {
                id: crypto.randomUUID(),
                project_id: projectId,
                line_order: afterIndex + 1,
                text: '',
                character_id: null,
            };
            const newLines = [...state.lines];
            newLines.splice(afterIndex + 1, 0, newLine);
            // Re-index line_order
            return {
                lines: newLines.map((l, i) => ({ ...l, line_order: i })),
                isDirty: true,
            };
        });
    },

    deleteLine: (lineId: string) => {
        set((state) => {
            const filtered = state.lines.filter((l) => l.id !== lineId);
            return {
                lines: filtered.map((l, i) => ({ ...l, line_order: i })),
                isDirty: true,
            };
        });
    },

    reorderLines: (fromIndex: number, toIndex: number) => {
        set((state) => {
            const newLines = [...state.lines];
            const [moved] = newLines.splice(fromIndex, 1);
            newLines.splice(toIndex, 0, moved);
            return {
                lines: newLines.map((l, i) => ({ ...l, line_order: i })),
                isDirty: true,
            };
        });
    },

    saveScript: async () => {
        const { useProjectStore } = await import('./projectStore');
        const projectId = useProjectStore.getState().currentProject?.project.id;
        if (!projectId) return;
        try {
            await ipc.saveScript(projectId, get().lines);
            set({ isDirty: false });
        } catch (e) {
            console.error('Failed to save script:', e);
        }
    },
}));
