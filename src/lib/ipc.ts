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
    try {
        return await invoke<T>(command, args);
    } catch (error) {
        // Re-throw with structured context for upstream consumers
        throw error;
    }
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

export async function importCharacters(
    toProjectId: string,
    characterIds: string[],
): Promise<Character[]> {
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
    enableThinking: boolean = false,
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
    enableThinking: boolean,
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
    return listen<string>('llm-token', (event) => callback(event.payload));
}

export function onLlmThinking(callback: (token: string) => void): Promise<UnlistenFn> {
    return listen<string>('llm-thinking', (event) => callback(event.payload));
}

export function onLlmCancel(callback: () => void): Promise<UnlistenFn> {
    return listen('llm-cancel', () => callback());
}

export function onLlmComplete(callback: () => void): Promise<UnlistenFn> {
    return listen('llm-complete', () => callback());
}

export function onLlmError(callback: (error: string) => void): Promise<UnlistenFn> {
    return listen<string>('llm-error', (event) => callback(event.payload));
}

export async function cancelLlm(): Promise<void> {
    return ipcCall<void>('cancel_llm');
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
    instructions?: string,
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

export async function generateAllTts(
    projectId: string,
    apiKey: string,
): Promise<number> {
    return ipcCall<number>('generate_all_tts', {
        projectId,
        apiKey,
    });
}

export function onTtsBatchProgress(callback: (progress: TtsBatchProgress) => void): Promise<UnlistenFn> {
    return listen<TtsBatchProgress>('tts-batch-progress', (event) => callback(event.payload));
}

// ---- Audio Mix Export ----

export async function exportAudioMix(
    projectId: string,
    outputPath: string,
    bgmPath: string | null,
    bgmVolume: number,
): Promise<string> {
    return ipcCall<string>('export_audio_mix', {
        projectId,
        outputPath,
        bgmPath,
        bgmVolume,
    });
}

export function onMixProgress(callback: (progress: MixProgress) => void): Promise<UnlistenFn> {
    return listen<MixProgress>('mix-progress', (event) => callback(event.payload));
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
