import { create } from 'zustand';
import { temporal } from 'zundo';
import * as ipc from '../lib/ipc';
import { useProjectStore } from './projectStore';
import { useCharacterStore } from './characterStore';
import { useToastStore } from './toastStore';
import type { ScriptLine } from '../types';

interface ScriptStore {
    lines: ScriptLine[];
    isGenerating: boolean;
    isAnalyzing: boolean;
    isDirty: boolean;
    streamingText: string;
    isBatchTtsRunning: boolean;
    batchTtsProgress: { current: number; total: number } | null;
    agentPlan: ipc.AgentPlan | null;
    workflow: 'ai' | 'manual' | null;
    setWorkflow: (mode: 'ai' | 'manual' | null) => void;
    analyzeOutline: (outline: string) => Promise<void>;
    generateScript: (outline: string, extraInstructions?: string, confirmedCharNames?: string[]) => Promise<void>;
    setAgentPlan: (plan: ipc.AgentPlan | null) => void;
    updateLine: (lineId: string, text: string) => void;
    assignCharacter: (lineId: string, characterId: string) => void;
    addLine: (afterIndex: number) => void;
    deleteLine: (lineId: string) => void;
    reorderLines: (fromIndex: number, toIndex: number) => void;
    setGap: (lineId: string, gapMs: number) => void;
    setInstructions: (lineId: string, instructions: string) => void;
    saveScript: () => Promise<void>;
    generateAllTts: () => Promise<void>;
}

// Only track undo history for `lines` changes, not for UI states
const linesOnlyDiff = (
    past: Partial<Pick<ScriptStore, 'lines'>>,
    current: Partial<Pick<ScriptStore, 'lines'>>,
) => {
    return past.lines !== current.lines ? { lines: current.lines } : null;
};

export const useScriptStore = create<ScriptStore>()(
    temporal(
        (set, get) => ({
            lines: [],
            isGenerating: false,
            isAnalyzing: false,
            isDirty: false,
            streamingText: '',
            isBatchTtsRunning: false,
            batchTtsProgress: null,
            agentPlan: null,
            workflow: null,

            setWorkflow: (mode: 'ai' | 'manual' | null) => {
                set({ workflow: mode });
            },

            analyzeOutline: async (outline: string) => {
                const { useSettingsStore } = await import('./settingsStore');
                const project = useProjectStore.getState().currentProject;
                if (!project) return;

                const settings = useSettingsStore.getState();
                const apiKey = await ipc.loadApiKey('dashscope');
                const characters = useCharacterStore.getState().characters;

                set({ isAnalyzing: true, streamingText: '' });

                const unlistenToken = await ipc.onLlmToken((token) => {
                    set((state) => ({ streamingText: state.streamingText + token }));
                });

                const unlistenError = await ipc.onLlmError((_error) => {
                    useToastStore.getState().addToast('分析大纲失败');
                    set({ isAnalyzing: false, streamingText: '' });
                });

                try {
                    const plan = await ipc.analyzeOutline(outline, {
                        api_endpoint: settings.llmEndpoint,
                        api_key: apiKey ?? '',
                        model: settings.llmModel,
                    }, characters);
                    set({ agentPlan: plan, isAnalyzing: false, streamingText: '' });
                } catch (e) {
                    useToastStore.getState().addToast('分析大纲失败');
                    set({ isAnalyzing: false, streamingText: '' });
                } finally {
                    unlistenToken();
                    unlistenError();
                }
            },

            setAgentPlan: (plan: ipc.AgentPlan | null) => {
                set({ agentPlan: plan });
            },

            generateScript: async (outline: string, extraInstructions?: string, confirmedCharNames?: string[]) => {
                const { useSettingsStore } = await import('./settingsStore');
                const project = useProjectStore.getState().currentProject;
                if (!project) return;

                const settings = useSettingsStore.getState();
                const apiKey = await ipc.loadApiKey('dashscope');
                const allCharacters = useCharacterStore.getState().characters;

                // If confirmed character names are provided, only use those.
                // Otherwise fall back to all characters (for direct generate without plan).
                const characters = confirmedCharNames
                    ? allCharacters.filter((c) => confirmedCharNames.includes(c.name))
                    : allCharacters;

                // Preserve old lines in case generation fails
                const oldLines = get().lines;
                set({ isGenerating: true, streamingText: '' });

                const unlistenToken = await ipc.onLlmToken((token) => {
                    set((state) => ({ streamingText: state.streamingText + token }));
                });

                const unlistenComplete = await ipc.onLlmComplete(() => {
                    // Reload script lines from backend after generation completes
                    ipc.loadScript(project.project.id).then((lines) => {
                        set({ lines, isGenerating: false, streamingText: '', isDirty: false });
                    });
                });

                const unlistenError = await ipc.onLlmError((_error) => {
                    useToastStore.getState().addToast('AI 生成出错，已恢复旧内容');
                    set({ lines: oldLines, isGenerating: false, streamingText: '' });
                });

                try {
                    await ipc.generateScript(project.project.id, outline, {
                        api_endpoint: settings.llmEndpoint,
                        api_key: apiKey ?? '',
                        model: settings.llmModel,
                    }, characters, extraInstructions);
                } catch (e) {
                    useToastStore.getState().addToast('生成剧本失败');
                    set({ lines: oldLines, isGenerating: false, streamingText: '' });
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
                    // Get project_id from existing lines, or fall back to the current project in projectStore
                    const projectId = state.lines[0]?.project_id
                        || useProjectStore.getState().currentProject?.project.id
                        || '';
                    const newLine: ScriptLine = {
                        id: crypto.randomUUID(),
                        project_id: projectId,
                        line_order: afterIndex + 1,
                        text: '',
                        character_id: null,
                        gap_after_ms: 500,
                        instructions: '',
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

            setGap: (lineId: string, gapMs: number) => {
                set((state) => ({
                    lines: state.lines.map((l) =>
                        l.id === lineId ? { ...l, gap_after_ms: gapMs } : l,
                    ),
                    isDirty: true,
                }));
            },

            setInstructions: (lineId: string, instructions: string) => {
                set((state) => ({
                    lines: state.lines.map((l) =>
                        l.id === lineId ? { ...l, instructions } : l,
                    ),
                    isDirty: true,
                }));
            },

            saveScript: async () => {
                const projectId = useProjectStore.getState().currentProject?.project.id;
                if (!projectId) return;
                try {
                    await ipc.saveScript(projectId, get().lines);
                    set({ isDirty: false });
                } catch (e) {
                    useToastStore.getState().addToast('保存剧本失败');
                }
            },

            generateAllTts: async () => {
                const project = useProjectStore.getState().currentProject;
                if (!project) return;

                const apiKey = await ipc.loadApiKey('dashscope');
                if (!apiKey) return;

                set({ isBatchTtsRunning: true, batchTtsProgress: null });

                const unlisten = await ipc.onTtsBatchProgress((p) => {
                    set({ batchTtsProgress: { current: p.current, total: p.total } });
                });

                try {
                    await ipc.generateAllTts(project.project.id, apiKey);
                    // Reload project to refresh audio fragments
                    await useProjectStore.getState().loadProject(project.project.id);
                } catch (e) {
                    useToastStore.getState().addToast('批量生成语音失败');
                } finally {
                    unlisten();
                    set({ isBatchTtsRunning: false });
                }
            },
        }),
        {
            diff: linesOnlyDiff,
            limit: 50,
        },
    ),
);
