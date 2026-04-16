import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  Project,
  ProjectDetail,
  Character,
  CharacterInput,
  ScriptLine,
  ScriptSection,
  AudioFragment,
  VoiceConfig,
  LlmConfig,
  MixProgress,
  TtsBatchProgress,
  UserSettings,
} from '../types';

// ---- Unified IPC call wrapper with error handling ----

async function ipcCall<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return await invoke<T>(command, args);
}

// ---- Project Management ----

export async function createProject(name: string): Promise<Project> {
  return ipcCall<Project>('create_project', { name });
}

export async function listProjects(): Promise<Project[]> {
  return ipcCall<Project[]>('list_projects');
}

export async function loadProject(projectId: string): Promise<ProjectDetail> {
  return ipcCall<ProjectDetail>('load_project', { projectId });
}

export async function deleteProject(projectId: string): Promise<void> {
  return ipcCall<void>('delete_project', { projectId });
}

export async function saveOutline(projectId: string, outline: string): Promise<void> {
  return ipcCall<void>('save_outline', { projectId, outline });
}

// ---- Character Management ----

export async function createCharacter(projectId: string, input: CharacterInput): Promise<Character> {
  return ipcCall<Character>('create_character', { projectId, input });
}

export async function updateCharacter(characterId: string, input: CharacterInput): Promise<Character> {
  return ipcCall<Character>('update_character', { characterId, input });
}

export async function deleteCharacter(characterId: string): Promise<void> {
  return ipcCall<void>('delete_character', { characterId });
}

export async function listCharacters(projectId: string): Promise<Character[]> {
  return ipcCall<Character[]>('list_characters', { projectId });
}

export async function listAllProjectCharacters(): Promise<[string, string, Character[]][]> {
  return ipcCall<[string, string, Character[]][]>('list_all_project_characters');
}

export async function importCharacters(toProjectId: string, characterIds: string[]): Promise<Character[]> {
  return ipcCall<Character[]>('import_characters', { toProjectId, characterIds });
}

// ---- LLM Script Generation ----

export async function generateScript(
  projectId: string,
  outline: string,
  config: LlmConfig,
  characters: Character[],
  agentPlan?: AgentPlan | null,
  extraInstructions?: string,
  enableThinking: boolean = false
): Promise<void> {
  return ipcCall<void>('generate_script', {
    projectId,
    outline,
    apiEndpoint: config.api_endpoint,
    apiKey: config.api_key,
    model: config.model,
    characters,
    agentPlan: agentPlan || undefined,
    extraInstructions: extraInstructions || undefined,
    enableThinking,
  });
}

// ---- Agent: Outline Analysis (Phase 1) ----

export async function analyzeOutline(
  outline: string,
  config: LlmConfig,
  characters: Character[],
  enableThinking: boolean
): Promise<AgentPlan> {
  return ipcCall<AgentPlan>('analyze_outline', {
    outline,
    apiEndpoint: config.api_endpoint,
    apiKey: config.api_key,
    model: config.model,
    characters,
    enableThinking,
  });
}

export interface AgentPlan {
  chapters: ChapterPlan[];
  suggested_characters: SuggestedCharacter[];
  overall_style: string;
  character_notes: string;
}

export interface ChapterPlan {
  title: string;
  estimated_lines: number;
  characters: string[];
  mood: string;
}

export interface SuggestedCharacter {
  name: string;
  role: string;
  matched_existing: boolean;
  existing_id: string | null;
}

export function onLlmToken(callback: (token: string) => void): Promise<UnlistenFn> {
  return listen<string>('llm-token', (event) => {
    callback(event.payload);
  });
}

export function onLlmThinking(callback: (token: string) => void): Promise<UnlistenFn> {
  return listen<string>('llm-thinking', (event) => {
    callback(event.payload);
  });
}

export function onLlmCancel(callback: () => void): Promise<UnlistenFn> {
  return listen('llm-cancel', () => {
    callback();
  });
}

export function onLlmComplete(callback: () => void): Promise<UnlistenFn> {
  return listen('llm-complete', () => {
    callback();
  });
}

export function onLlmError(callback: (error: string) => void): Promise<UnlistenFn> {
  return listen<string>('llm-error', (event) => {
    callback(event.payload);
  });
}

export async function cancelLlm(): Promise<void> {
  return ipcCall<void>('cancel_llm');
}

// ---- Agent Pipeline ----

export async function runAgentPipeline(
  projectId: string,
  outline: string,
  config: LlmConfig,
  characters: Character[],
  agentPlan?: AgentPlan | null,
  extraInstructions?: string,
  enableThinking: boolean = false
): Promise<void> {
  return ipcCall<void>('run_agent_pipeline', {
    projectId,
    outline,
    apiEndpoint: config.api_endpoint,
    apiKey: config.api_key,
    model: config.model,
    characters,
    agentPlan: agentPlan || undefined,
    extraInstructions: extraInstructions || undefined,
    enableThinking,
  });
}

// ---- Step-based Agent Pipeline (Phase 1 → Confirm → Phase 2) ----

export async function runAnalysisStep(
  outline: string,
  config: LlmConfig,
  characters: Character[],
  enableThinking: boolean = false
): Promise<AgentPlan> {
  return ipcCall<AgentPlan>('run_analysis_step', {
    outline,
    apiEndpoint: config.api_endpoint,
    apiKey: config.api_key,
    model: config.model,
    characters,
    enableThinking,
  });
}

export interface NewCharacterInput {
  name: string;
  voice_name: string;
  tts_model: string;
  speed: number;
  pitch: number;
}

export async function runGenerationStep(
  projectId: string,
  outline: string,
  config: LlmConfig,
  characters: Character[],
  plan?: AgentPlan | null,
  extraInstructions?: string,
  enableThinking: boolean = false
): Promise<void> {
  return ipcCall<void>('run_generation_step', {
    projectId,
    outline,
    apiEndpoint: config.api_endpoint,
    apiKey: config.api_key,
    model: config.model,
    characters,
    plan: plan || undefined,
    extraInstructions: extraInstructions || undefined,
    enableThinking,
  });
}

export async function runRevisionStep(
  projectId: string,
  outline: string,
  config: LlmConfig,
  characters: Character[],
  instructions: string,
  sectionIndices?: number[],
  plan?: AgentPlan | null,
  enableThinking: boolean = false
): Promise<void> {
  return ipcCall<void>('run_revision_step', {
    projectId,
    outline,
    apiEndpoint: config.api_endpoint,
    apiKey: config.api_key,
    model: config.model,
    characters,
    instructions,
    sectionIndices: sectionIndices || undefined,
    plan: plan || undefined,
    enableThinking,
  });
}

// ---- Agent Pipeline Events ----

export function onAgentPipelineStarted(
  callback: (data: { project_id: string; phase?: string; sections?: number[] }) => void
): Promise<UnlistenFn> {
  return listen<{ project_id: string; phase?: string; sections?: number[] }>('agent-pipeline-started', (event) => {
    callback(event.payload);
  });
}

export function onAgentStepComplete(
  callback: (data: { phase: string; plan?: AgentPlan; project_id: string }) => void
): Promise<UnlistenFn> {
  return listen<{ phase: string; plan?: AgentPlan; project_id: string }>('agent-step-complete', (event) => {
    callback(event.payload);
  });
}

export function onAgentSectionGenerated(
  callback: (data: {
    section_index: number;
    section_id: string;
    title: string;
    line_count: number;
    characters: string[];
  }) => void
): Promise<UnlistenFn> {
  return listen<{ section_index: number; section_id: string; title: string; line_count: number; characters: string[] }>(
    'agent-section-generated',
    (event) => {
      callback(event.payload);
    }
  );
}

export function onAgentValidation(
  callback: (data: {
    total_lines: number;
    total_sections: number;
    unmatched_characters: string[];
    warnings: string[];
  }) => void
): Promise<UnlistenFn> {
  return listen<{ total_lines: number; total_sections: number; unmatched_characters: string[]; warnings: string[] }>(
    'agent-validation',
    (event) => {
      callback(event.payload);
    }
  );
}

export function onAgentTtsSuggestions(
  callback: (data: {
    suggestions: { character_name: string; suggested_speed: number; suggested_pitch: number; reason: string }[];
  }) => void
): Promise<UnlistenFn> {
  return listen<{
    suggestions: { character_name: string; suggested_speed: number; suggested_pitch: number; reason: string }[];
  }>('agent-tts-suggestions', (event) => {
    callback(event.payload);
  });
}

export function onAgentRetry(
  callback: (data: { attempt: number; max_retries: number; reason: string }) => void
): Promise<UnlistenFn> {
  return listen<{ attempt: number; max_retries: number; reason: string }>('agent-retry', (event) => {
    callback(event.payload);
  });
}

// ---- Story Knowledge Base (Vector Search) ----

export interface StoryRecallResult {
  text: string;
  kb_type: string;
  score: number;
  metadata: string;
}

export async function storyRecall(
  projectId: string,
  query: string,
  apiEndpoint: string,
  apiKey: string,
  model: string,
  kbType?: string,
  topK?: number,
  enableThinking: boolean = false
): Promise<StoryRecallResult[]> {
  return ipcCall<StoryRecallResult[]>('story_recall', {
    projectId,
    query,
    apiEndpoint,
    apiKey,
    model,
    kbType: kbType || undefined,
    topK: topK || undefined,
    enableThinking,
  });
}

export async function buildStoryKb(
  projectId: string,
  apiEndpoint: string,
  apiKey: string,
  model: string
): Promise<number> {
  return ipcCall<number>('build_story_kb', {
    projectId,
    apiEndpoint,
    apiKey,
    model,
  });
}

export function onAgentKbIndexed(callback: (data: { section: string; lines: number }) => void): Promise<UnlistenFn> {
  return listen<{ section: string; lines: number }>('agent-kb-indexed', (event) => {
    callback(event.payload);
  });
}

export function onAgentToolCall(
  callback: (data: { tool: string; query?: string; action?: string; text?: string }) => void
): Promise<UnlistenFn> {
  return listen<{ tool: string; query?: string; action?: string; text?: string }>('agent-tool-call', (event) => {
    callback(event.payload);
  });
}

export function onAgentToolResult(
  callback: (data: { tool: string; results_count?: number; id?: string }) => void
): Promise<UnlistenFn> {
  return listen<{ tool: string; results_count?: number; id?: string }>('agent-tool-result', (event) => {
    callback(event.payload);
  });
}

// ---- Script Operations ----

export async function saveScript(projectId: string, lines: ScriptLine[], sections: ScriptSection[]): Promise<void> {
  return ipcCall<void>('save_script', { projectId, lines, sections });
}

export async function loadScript(projectId: string): Promise<ScriptLine[]> {
  return ipcCall<ScriptLine[]>('load_script', { projectId });
}

// ---- TTS Voice Generation ----

export async function generateTts(
  projectId: string,
  lineId: string,
  text: string,
  voiceConfig: VoiceConfig,
  apiKey: string,
  instructions?: string
): Promise<AudioFragment> {
  return ipcCall<AudioFragment>('generate_tts', {
    projectId,
    lineId,
    text,
    voiceConfig,
    apiKey,
    instructions: instructions || undefined,
  });
}

export async function generateAllTts(projectId: string, apiKey: string): Promise<number> {
  return ipcCall<number>('generate_all_tts', {
    projectId,
    apiKey,
  });
}

export async function clearAudioFragments(projectId: string): Promise<void> {
  return ipcCall<void>('clear_audio_fragments', { projectId });
}

export async function clearTtsFragments(projectId: string): Promise<void> {
  return ipcCall<void>('clear_tts_fragments', { projectId });
}

export async function importAudio(projectId: string, lineId: string, audioDataBase64: string): Promise<AudioFragment> {
  return ipcCall<AudioFragment>('import_audio', {
    projectId,
    lineId,
    audioDataBase64,
  });
}

export function onTtsBatchProgress(callback: (progress: TtsBatchProgress) => void): Promise<UnlistenFn> {
  return listen<TtsBatchProgress>('tts-batch-progress', (event) => {
    callback(event.payload);
  });
}

// ---- Audio Mix Export ----

export async function exportAudioMix(
  projectId: string,
  outputPath: string,
  bgmPath: string | null,
  bgmVolume: number
): Promise<string> {
  return ipcCall<string>('export_audio_mix', {
    projectId,
    outputPath,
    bgmPath,
    bgmVolume,
  });
}

export function onMixProgress(callback: (progress: MixProgress) => void): Promise<UnlistenFn> {
  return listen<MixProgress>('mix-progress', (event) => {
    callback(event.payload);
  });
}

// ---- Settings Management ----

export async function saveSettings(settings: UserSettings): Promise<void> {
  return ipcCall<void>('save_settings', { settings });
}

export async function loadSettings(): Promise<UserSettings> {
  return ipcCall<UserSettings>('load_settings');
}

// ---- API Key Management ----

export async function saveApiKey(service: string, key: string): Promise<void> {
  return ipcCall<void>('save_api_key', { service, key });
}

export async function loadApiKey(service: string): Promise<string | null> {
  return ipcCall<string | null>('load_api_key', { service });
}

// ---- Auto Update ----

export interface UpdateInfo {
  available: boolean;
  version: string;
  body: string | null;
}

export async function checkForUpdates(): Promise<UpdateInfo> {
  return ipcCall<UpdateInfo>('check_for_updates');
}

export async function installUpdate(): Promise<void> {
  return ipcCall<void>('install_update');
}
