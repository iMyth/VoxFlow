export interface Project {
  id: string;
  name: string;
  outline: string;
  created_at: string;
  updated_at: string;
}

export interface ProjectDetail {
  project: Project;
  characters: Character[];
  sections: ScriptSection[];
  script_lines: ScriptLine[];
  audio_fragments: AudioFragment[];
}

export interface Character {
  id: string;
  project_id: string;
  name: string;
  voice_name: string;
  tts_model: string;
  speed: number;
  pitch: number;
}

export interface ScriptLine {
  id: string;
  project_id: string;
  line_order: number;
  text: string;
  character_id: string | null;
  gap_after_ms: number;
  instructions: string;
  section_id: string | null;
}

export interface ScriptSection {
  id: string;
  project_id: string;
  title: string;
  section_order: number;
}

export interface AudioFragment {
  id: string;
  project_id: string;
  line_id: string;
  file_path: string;
  duration_ms: number | null;
}

export interface VoiceConfig {
  voice_name: string;
  tts_model: string;
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

export interface TtsBatchProgress {
  current: number;
  total: number;
  line_id: string;
  success: boolean;
  error: string | null;
}

export interface UserSettings {
  llm_endpoint: string;
  llm_model: string;
  default_tts_model: string;
  default_voice_name: string;
  default_speed: number;
  default_pitch: number;
  enable_thinking: boolean;
}

export interface CharacterInput {
  name: string;
  voice_name: string;
  tts_model: string;
  speed: number;
  pitch: number;
}
