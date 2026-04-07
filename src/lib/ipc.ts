import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
    Project,
    ProjectDetail,
    Character,
    CharacterInput,
    ScriptLine,
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

// ---- LLM Script Generation ----

export async function generateScript(
    projectId: string,
    outline: string,
    config: LlmConfig,
    characters: Character[],
): Promise<void> {
    return ipcCall<void>('generate_script', {
        projectId,
        outline,
        apiEndpoint: config.api_endpoint,
        apiKey: config.api_key,
        model: config.model,
        characters,
    });
}

export function onLlmToken(callback: (token: string) => void): Promise<UnlistenFn> {
    return listen<string>('llm-token', (event) => callback(event.payload));
}

export function onLlmComplete(callback: () => void): Promise<UnlistenFn> {
    return listen('llm-complete', () => callback());
}

export function onLlmError(callback: (error: string) => void): Promise<UnlistenFn> {
    return listen<string>('llm-error', (event) => callback(event.payload));
}

// ---- Script Operations ----

export async function saveScript(projectId: string, lines: ScriptLine[]): Promise<void> {
    return ipcCall<void>('save_script', { projectId, lines });
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
): Promise<AudioFragment> {
    return ipcCall<AudioFragment>('generate_tts', {
        projectId,
        lineId,
        text,
        voiceConfig,
        apiKey,
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
