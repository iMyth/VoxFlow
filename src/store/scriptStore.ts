import { temporal } from 'zundo';
import { create } from 'zustand';

import { useCharacterStore } from './characterStore';
import { useProjectStore } from './projectStore';
import { useToastStore } from './toastStore';
import * as ipc from '../lib/ipc';

import type { ScriptLine, ScriptSection } from '../types';

// ---- Batched streaming text updates via rAF to avoid per-token re-renders ----
let textBuf = '';
let thinkBuf = '';
let textRaf: number | null = null;
let thinkRaf: number | null = null;

function flushText(set: (u: Partial<ScriptStore>) => void) {
  if (textBuf) {
    set({ streamingText: textBuf });
  }
  textRaf = null;
}

function flushThink(set: (u: Partial<ScriptStore>) => void) {
  if (thinkBuf) {
    set({ thinkingText: thinkBuf });
  }
  thinkRaf = null;
}

function queueText(set: (u: Partial<ScriptStore>) => void, token: string) {
  textBuf += token;
  if (!textRaf) {
    textRaf = requestAnimationFrame(() => {
      flushText(set);
    });
  }
}

function queueThink(set: (u: Partial<ScriptStore>) => void, token: string) {
  thinkBuf += token;
  if (!thinkRaf) {
    thinkRaf = requestAnimationFrame(() => {
      flushThink(set);
    });
  }
}

function makeToolEntry(data: { tool: string; query?: string; action?: string; text?: string }): ToolCallEntry {
  const args: Record<string, unknown> = {};
  if (data.query) args.query = data.query;
  if (data.action) args.action = data.action;
  if (data.text) args.text = data.text;
  return {
    id: `${Date.now()}-${data.tool}`,
    tool: data.tool,
    args,
    status: 'calling',
    timestamp: Date.now(),
  };
}

function markToolDone(calls: ToolCallEntry[], tool: string, result: string): ToolCallEntry[] {
  for (let i = calls.length - 1; i >= 0; i--) {
    if (calls[i].tool === tool && calls[i].status === 'calling') {
      calls[i] = { ...calls[i], status: 'done', result };
      break;
    }
  }
  return calls;
}

export interface ToolCallEntry {
  id: string;
  tool: string;
  args: Record<string, unknown>;
  status: 'calling' | 'done' | 'error';
  result?: string;
  timestamp: number;
}

interface ScriptStore {
  lines: ScriptLine[];
  sections: ScriptSection[];
  isGenerating: boolean;
  isAnalyzing: boolean;
  isDirty: boolean;
  streamingText: string;
  thinkingText: string;
  toolCalls: ToolCallEntry[];
  enableThinking: boolean;
  setEnableThinking: (v: boolean) => void;
  isBatchTtsRunning: boolean;
  batchTtsProgress: { current: number; total: number } | null;
  agentPlan: ipc.AgentPlan | null;
  workflow: 'ai' | 'manual' | null;
  setWorkflow: (mode: 'ai' | 'manual' | null) => void;
  analyzeOutline: (outline: string) => Promise<void>;
  generateScript: (outline: string, extraInstructions?: string, confirmedCharNames?: string[]) => Promise<void>;
  runAgentPipeline: (outline: string, extraInstructions?: string) => Promise<void>;
  cancelLlm: () => Promise<void>;
  setAgentPlan: (plan: ipc.AgentPlan | null) => void;
  updateLine: (lineId: string, text: string) => void;
  assignCharacter: (lineId: string, characterId: string) => void;
  addLine: (afterIndex: number, sectionId?: string) => void;
  deleteLine: (lineId: string) => void;
  reorderLines: (fromIndex: number, toIndex: number) => void;
  setGap: (lineId: string, gapMs: number) => void;
  setInstructions: (lineId: string, instructions: string) => void;
  setAllInstructions: (instructions: string) => void;
  addSection: () => void;
  deleteSection: (sectionId: string) => void;
  renameSection: (sectionId: string, title: string) => void;
  reorderSections: (fromIndex: number, toIndex: number) => void;
  moveLineToSection: (lineId: string, sectionId: string | null) => void;
  saveScript: () => Promise<void>;
  generateAllTts: () => Promise<void>;
  regenerateAllTts: (preserveRecordings?: boolean) => Promise<void>;
}

// Only track undo history for `lines` changes, not for UI states
const linesOnlyDiff = (past: Partial<Pick<ScriptStore, 'lines'>>, current: Partial<Pick<ScriptStore, 'lines'>>) => {
  return past.lines !== current.lines ? { lines: current.lines } : null;
};

export const useScriptStore = create<ScriptStore>()(
  temporal(
    (set, get) => ({
      lines: [],
      sections: [],
      isGenerating: false,
      isAnalyzing: false,
      isDirty: false,
      streamingText: '',
      thinkingText: '',
      toolCalls: [],
      enableThinking: true,
      isBatchTtsRunning: false,
      batchTtsProgress: null,
      agentPlan: null,
      workflow: null,

      setEnableThinking: (v: boolean) => {
        set({ enableThinking: v });
        // Persist to settings
        void import('./settingsStore').then(({ useSettingsStore }) => {
          useSettingsStore.getState().set({ enableThinking: v });
          void useSettingsStore.getState().saveSettings();
        });
      },

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
        // Read persisted settings value
        const enableThinking = settings.enableThinking;

        textBuf = '';
        thinkBuf = '';
        set({ isAnalyzing: true, streamingText: '', thinkingText: '' });

        const unlistenToken = await ipc.onLlmToken((token) => {
          queueText(set, token);
        });

        const unlistenThinking = await ipc.onLlmThinking((token) => {
          queueThink(set, token);
        });

        const unlistenToolCall = await ipc.onAgentToolCall((data) => {
          set((prev) => ({ toolCalls: [...prev.toolCalls, makeToolEntry(data)] }));
        });

        const unlistenToolResult = await ipc.onAgentToolResult((data) => {
          set((prev) => ({
            toolCalls: markToolDone([...prev.toolCalls], data.tool, `${String(data.results_count ?? '?')} results`),
          }));
        });

        const unlistenCancel = await ipc.onLlmCancel(() => {
          flushText(set);
          flushThink(set);
          useToastStore.getState().addToast('editor.cancelAnalyzeToast', 'info');
          set({ isAnalyzing: false, streamingText: '', thinkingText: '', toolCalls: [] });
        });

        const unlistenError = await ipc.onLlmError((_error) => {
          flushText(set);
          flushThink(set);
          useToastStore.getState().addToast('editor.analyzeFailed');
          set({ isAnalyzing: false, streamingText: '', thinkingText: '', toolCalls: [] });
        });

        try {
          const plan = await ipc.analyzeOutline(
            outline,
            {
              api_endpoint: settings.llmEndpoint,
              api_key: apiKey ?? '',
              model: settings.llmModel,
            },
            characters,
            enableThinking
          );
          flushText(set);
          flushThink(set);
          set({ agentPlan: plan, isAnalyzing: false, streamingText: '', thinkingText: '', toolCalls: [] });
        } catch (e) {
          const errMsg = String(e);
          if (!errMsg.includes('已取消')) {
            useToastStore.getState().addToast('editor.analyzeFailed');
          }
          flushText(set);
          flushThink(set);
          set({ isAnalyzing: false, streamingText: '', thinkingText: '', toolCalls: [] });
        } finally {
          unlistenToken();
          unlistenThinking();
          unlistenToolCall();
          unlistenToolResult();
          unlistenCancel();
          unlistenError();
        }
      },

      cancelLlm: async () => {
        await ipc.cancelLlm();
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
        // Read persisted settings value
        const enableThinking = settings.enableThinking;
        const agentPlan = get().agentPlan;

        const characters = confirmedCharNames
          ? allCharacters.filter((c) => confirmedCharNames.includes(c.name))
          : allCharacters;

        const oldLines = get().lines;
        textBuf = '';
        thinkBuf = '';
        set({ isGenerating: true, streamingText: '', thinkingText: '', toolCalls: [] });

        const unlistenToken = await ipc.onLlmToken((token) => {
          queueText(set, token);
        });

        const unlistenThinking = await ipc.onLlmThinking((token) => {
          queueThink(set, token);
        });

        const unlistenToolCall = await ipc.onAgentToolCall((data) => {
          set((prev) => ({ toolCalls: [...prev.toolCalls, makeToolEntry(data)] }));
        });

        const unlistenToolResult = await ipc.onAgentToolResult((data) => {
          set((prev) => ({
            toolCalls: markToolDone([...prev.toolCalls], data.tool, `${String(data.results_count ?? '?')} results`),
          }));
        });

        const unlistenCancel = await ipc.onLlmCancel(() => {
          useToastStore.getState().addToast('editor.cancelGenerate', 'info');
          set({ lines: oldLines, isGenerating: false, streamingText: '', thinkingText: '', toolCalls: [] });
        });

        const unlistenComplete = await ipc.onLlmComplete(() => {
          // Reload full project to get both lines and sections
          void useProjectStore
            .getState()
            .loadProject(project.project.id)
            .then(() => {
              const updated = useProjectStore.getState().currentProject;
              if (updated) {
                set({
                  lines: updated.script_lines,
                  sections: updated.sections,
                  isGenerating: false,
                  streamingText: '',
                  thinkingText: '',
                  isDirty: false,
                });
              } else {
                set({
                  lines: [],
                  sections: [],
                  isGenerating: false,
                  streamingText: '',
                  thinkingText: '',
                  isDirty: false,
                });
              }
            });
        });

        const unlistenError = await ipc.onLlmError((_error) => {
          useToastStore.getState().addToast('editor.generateErrorRecovered');
          set({ lines: oldLines, isGenerating: false, streamingText: '', thinkingText: '' });
        });

        try {
          await ipc.generateScript(
            project.project.id,
            outline,
            {
              api_endpoint: settings.llmEndpoint,
              api_key: apiKey ?? '',
              model: settings.llmModel,
            },
            characters,
            agentPlan,
            extraInstructions,
            enableThinking
          );
        } catch (_e) {
          useToastStore.getState().addToast('editor.generateFailed');
          set({ lines: oldLines, isGenerating: false, streamingText: '', thinkingText: '', toolCalls: [] });
        } finally {
          unlistenToken();
          unlistenThinking();
          unlistenToolCall();
          unlistenToolResult();
          unlistenCancel();
          unlistenComplete();
          unlistenError();
        }
      },

      runAgentPipeline: async (outline: string, extraInstructions?: string) => {
        const { useSettingsStore } = await import('./settingsStore');
        const project = useProjectStore.getState().currentProject;
        if (!project) return;

        const settings = useSettingsStore.getState();
        const apiKey = await ipc.loadApiKey('dashscope');
        const characters = useCharacterStore.getState().characters;
        const enableThinking = settings.enableThinking;
        const agentPlan = get().agentPlan;

        const oldLines = get().lines;
        textBuf = '';
        thinkBuf = '';
        set({ isGenerating: true, streamingText: '', thinkingText: '', toolCalls: [] });

        const unlistenToken = await ipc.onLlmToken((token) => {
          queueText(set, token);
        });

        const unlistenThinking = await ipc.onLlmThinking((token) => {
          queueThink(set, token);
        });

        const unlistenToolCall = await ipc.onAgentToolCall((data) => {
          set((prev) => ({ toolCalls: [...prev.toolCalls, makeToolEntry(data)] }));
        });

        const unlistenToolResult = await ipc.onAgentToolResult((data) => {
          set((prev) => ({
            toolCalls: markToolDone([...prev.toolCalls], data.tool, `${String(data.results_count ?? '?')} results`),
          }));
        });

        const unlistenCancel = await ipc.onLlmCancel(() => {
          useToastStore.getState().addToast('editor.cancelGenerate', 'info');
          set({ lines: oldLines, isGenerating: false, streamingText: '', thinkingText: '', toolCalls: [] });
        });

        const unlistenComplete = await ipc.onLlmComplete(() => {
          // Reload full project to get both lines and sections
          void useProjectStore
            .getState()
            .loadProject(project.project.id)
            .then(() => {
              const updated = useProjectStore.getState().currentProject;
              if (updated) {
                set({
                  lines: updated.script_lines,
                  sections: updated.sections,
                  isGenerating: false,
                  streamingText: '',
                  thinkingText: '',
                  isDirty: false,
                });
              } else {
                set({
                  lines: [],
                  sections: [],
                  isGenerating: false,
                  streamingText: '',
                  thinkingText: '',
                  isDirty: false,
                });
              }
            });
        });

        const unlistenError = await ipc.onLlmError((_error) => {
          useToastStore.getState().addToast('editor.generateErrorRecovered');
          set({ lines: oldLines, isGenerating: false, streamingText: '', thinkingText: '', toolCalls: [] });
        });

        try {
          await ipc.runAgentPipeline(
            project.project.id,
            outline,
            {
              api_endpoint: settings.llmEndpoint,
              api_key: apiKey ?? '',
              model: settings.llmModel,
            },
            characters,
            agentPlan,
            extraInstructions,
            enableThinking
          );
        } catch (_e) {
          useToastStore.getState().addToast('editor.generateFailed');
          set({ lines: oldLines, isGenerating: false, streamingText: '', thinkingText: '', toolCalls: [] });
        } finally {
          unlistenToken();
          unlistenThinking();
          unlistenToolCall();
          unlistenToolResult();
          unlistenCancel();
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
          lines: state.lines.map((l) => (l.id === lineId ? { ...l, character_id: characterId } : l)),
          isDirty: true,
        }));
      },

      addLine: (afterIndex: number, sectionId?: string) => {
        set((state) => {
          const projectId = state.lines[0]?.project_id || useProjectStore.getState().currentProject?.project.id || '';
          const newLine: ScriptLine = {
            id: crypto.randomUUID(),
            project_id: projectId,
            line_order: afterIndex + 1,
            text: '',
            character_id: null,
            gap_after_ms: 500,
            instructions: '',
            section_id: sectionId ?? state.lines[afterIndex]?.section_id ?? null,
          };

          // If afterIndex is -1 and sectionId is given, append to end of that section
          let insertIndex = afterIndex + 1;
          if (afterIndex < 0 && sectionId) {
            const lastInSection = state.lines
              .filter((l) => l.section_id === sectionId)
              .reduce((max, l) => Math.max(max, l.line_order), -1);
            insertIndex = lastInSection >= 0 ? lastInSection + 1 : 0;
          }

          const newLines = [...state.lines];
          newLines.splice(insertIndex, 0, newLine);
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
          lines: state.lines.map((l) => (l.id === lineId ? { ...l, gap_after_ms: gapMs } : l)),
          isDirty: true,
        }));
      },

      setInstructions: (lineId: string, instructions: string) => {
        set((state) => ({
          lines: state.lines.map((l) => (l.id === lineId ? { ...l, instructions } : l)),
          isDirty: true,
        }));
      },

      setAllInstructions: (instructions: string) => {
        set((state) => ({
          lines: state.lines.map((l) => ({ ...l, instructions })),
          isDirty: true,
        }));
      },

      addSection: () => {
        set((state) => {
          const projectId = state.lines[0]?.project_id || useProjectStore.getState().currentProject?.project.id || '';
          const newSection: ScriptSection = {
            id: crypto.randomUUID(),
            project_id: projectId,
            title: '新段落',
            section_order: state.sections.length,
          };
          return {
            sections: [...state.sections, newSection],
            isDirty: true,
          };
        });
      },

      deleteSection: (sectionId: string) => {
        set((state) => ({
          sections: state.sections.filter((s) => s.id !== sectionId),
          lines: state.lines.map((l) => (l.section_id === sectionId ? { ...l, section_id: null } : l)),
          isDirty: true,
        }));
      },

      renameSection: (sectionId: string, title: string) => {
        set((state) => ({
          sections: state.sections.map((s) => (s.id === sectionId ? { ...s, title } : s)),
          isDirty: true,
        }));
      },

      reorderSections: (fromIndex: number, toIndex: number) => {
        set((state) => {
          const newSections = [...state.sections];
          const [moved] = newSections.splice(fromIndex, 1);
          newSections.splice(toIndex, 0, moved);
          return {
            sections: newSections.map((s, i) => ({ ...s, section_order: i })),
            isDirty: true,
          };
        });
      },

      moveLineToSection: (lineId: string, sectionId: string | null) => {
        set((state) => ({
          lines: state.lines.map((l) => (l.id === lineId ? { ...l, section_id: sectionId } : l)),
          isDirty: true,
        }));
      },

      saveScript: async () => {
        const projectId = useProjectStore.getState().currentProject?.project.id;
        if (!projectId) return;
        try {
          await ipc.saveScript(projectId, get().lines, get().sections);
          set({ isDirty: false });
        } catch (_e) {
          useToastStore.getState().addToast('editor.saveScriptFailed');
        }
      },

      generateAllTts: async () => {
        const project = useProjectStore.getState().currentProject;
        if (!project) return;

        const apiKey = await ipc.loadApiKey('dashscope');
        if (!apiKey) return;

        // Compute total missing count upfront so UI shows immediately
        const audioFragments = useProjectStore.getState().currentProject?.audio_fragments ?? [];
        const coveredLineIds = new Set(audioFragments.map((a) => a.line_id));
        const totalMissing = get().lines.filter((l) => l.text.trim() && !coveredLineIds.has(l.id)).length;

        set({ isBatchTtsRunning: true, batchTtsProgress: { current: 0, total: totalMissing } });

        const unlisten = await ipc.onTtsBatchProgress((p) => {
          set({ batchTtsProgress: { current: p.current, total: p.total } });
        });

        try {
          await ipc.generateAllTts(project.project.id, apiKey);
          // Reload project to refresh audio fragments
          await useProjectStore.getState().loadProject(project.project.id);
        } catch (_e) {
          useToastStore.getState().addToast('editor.batchTtsFailed');
        } finally {
          unlisten();
          set({ isBatchTtsRunning: false });
        }
      },

      regenerateAllTts: async (preserveRecordings?: boolean) => {
        const project = useProjectStore.getState().currentProject;
        if (!project) return;

        try {
          if (preserveRecordings) {
            await ipc.clearTtsFragments(project.project.id);
          } else {
            await ipc.clearAudioFragments(project.project.id);
          }
          await useProjectStore.getState().loadProject(project.project.id);
        } catch (_e) {
          useToastStore.getState().addToast('editor.clearAudioFailed');
          return;
        }

        // Now run generateAllTts which will see all non-recording lines as missing
        await get().generateAllTts();
      },
    }),
    {
      diff: linesOnlyDiff,
      limit: 50,
    }
  )
);
