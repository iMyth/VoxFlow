export interface Project {
    id: string;
    name: string;
    created_at: string;
    updated_at: string;
}

export interface ProjectDetail {
    project: Project;
    characters: Character[];
    script_lines: ScriptLine[];
    audio_fragments: AudioFragment[];
}

export interface Character {
    id: string;
    project_id: string;
    name: string;
    tts_engine: 'edge-tts' | 'azure-tts' | 'dashscope-tts';
    voice_name: string;
    speed: number;
    pitch: number;
}

export interface ScriptLine {
    id: string;
    project_id: string;
    line_order: number;
    text: string;
    character_id: string | null;
}

export interface AudioFragment {
    id: string;
    project_id: string;
    line_id: string;
    file_path: string;
    duration_ms: number | null;
}

export interface VoiceConfig {
    engine: 'edge-tts' | 'azure-tts' | 'dashscope-tts';
    voice_name: string;
    speed: number;
    pitch: number;
}

export interface LlmConfig {
    api_endpoint: string;
    api_key: string;
    model: string;
}

export interface MixConfig {
    fragment_paths: string[];
    bgm_path: string | null;
    bgm_volume: number;
    output_path: string;
}

export interface MixProgress {
    percent: number;
    stage: string;
}

export interface UserSettings {
    llm_endpoint: string;
    llm_model: string;
    default_tts_engine: 'edge-tts' | 'azure-tts' | 'dashscope-tts';
    default_voice_name: string;
    default_speed: number;
    default_pitch: number;
}

export interface CharacterInput {
    name: string;
    tts_engine: 'edge-tts' | 'azure-tts' | 'dashscope-tts';
    voice_name: string;
    speed: number;
    pitch: number;
}
