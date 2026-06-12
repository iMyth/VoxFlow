use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;

use vox_flow_lib::core::agent::tools::outline_analysis::do_outline_analysis;
use vox_flow_lib::core::agent::tools::script_generation::do_script_generation;
use vox_flow_lib::core::db::Database;
use vox_flow_lib::core::event_emitter::LogEmitter;
use vox_flow_lib::core::models::{AudioFragment, Character, Project};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// ============================================================
// CLI definition
// ============================================================

#[derive(Parser)]
#[command(
    name = "voxflow",
    about = "VoxFlow CLI — audiobook production agent for other AI agents",
    version,
    long_about = None
)]
struct Cli {
    /// Path to the SQLite database file
    #[arg(short, long, env = "VOXFLOW_DB", default_value = "voxflow.db")]
    db: PathBuf,

    /// LLM API endpoint (OpenAI-compatible)
    #[arg(short, long, env = "VOXFLOW_API_ENDPOINT")]
    endpoint: Option<String>,

    /// LLM API key
    #[arg(short, long, env = "VOXFLOW_API_KEY")]
    key: Option<String>,

    /// LLM model name
    #[arg(short, long, env = "VOXFLOW_MODEL", default_value = "qwen-plus")]
    model: Option<String>,

    /// Enable LLM thinking/reasoning output
    #[arg(long, default_value = "false")]
    enable_thinking: bool,

    /// Verbose output (show all events)
    #[arg(short, long)]
    verbose: bool,

    /// Output format for list/info commands
    #[arg(short, long, default_value = "text")]
    format: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // ---- Project management ----
    #[command(about = "Project management")]
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },

    // ---- Character management ----
    #[command(about = "Character management")]
    Character {
        #[command(subcommand)]
        cmd: CharacterCmd,
    },

    // ---- Agent / LLM ----
    #[command(about = "Run agent pipeline (analyze + generate)")]
    Pipeline {
        /// Path to the outline text file
        #[arg(short, long)]
        outline: PathBuf,
        /// Project ID to save the script to
        #[arg(short, long)]
        project_id: String,
    },
    /// Generate script directly without analysis step
    Generate {
        /// Path to the outline text file
        #[arg(short, long)]
        outline: PathBuf,
        /// Project ID to save the script to
        #[arg(short, long)]
        project_id: String,
        /// Extra instructions for generation
        #[arg(long)]
        extra: Option<String>,
    },
    /// Run only the analysis step (returns JSON plan)
    Analyze {
        /// Path to the outline text file
        #[arg(short, long)]
        outline: PathBuf,
        /// Project ID (for loading existing characters)
        #[arg(short, long)]
        project_id: String,
    },
    /// Revise specific sections of an existing script
    Revise {
        /// Project ID
        project_id: String,
        /// Revision instructions
        #[arg(short, long)]
        instructions: String,
        /// Section indices to revise (0-based, comma-separated)
        #[arg(long)]
        sections: Option<String>,
        /// Path to the original outline file (optional, uses project outline if omitted)
        #[arg(short, long)]
        outline: Option<PathBuf>,
    },

    // ---- TTS ----
    #[command(about = "Generate TTS audio for all script lines")]
    Tts {
        /// Project ID
        project_id: String,
        /// TTS API key (defaults to LLM key if not set)
        #[arg(long)]
        tts_key: Option<String>,
    },

    // ---- Audio export ----
    #[command(about = "Export mixed audio (voice + BGM)")]
    Export {
        /// Project ID
        project_id: String,
        /// Output file path
        #[arg(short, long)]
        output: PathBuf,
        /// BGM file path (optional)
        #[arg(long)]
        bgm: Option<PathBuf>,
        /// BGM volume (0.0-1.0, default 0.3)
        #[arg(long, default_value = "0.3")]
        bgm_volume: Option<f32>,
    },
    /// Export script to stdout
    Script {
        /// Project ID
        project_id: String,
        /// Output format: text or json
        #[arg(short, long, default_value = "text")]
        format: Option<String>,
    },
    /// List audio fragments for a project
    AudioStatus {
        /// Project ID
        project_id: String,
    },
}

// ============================================================
// Project sub-commands
// ============================================================

#[derive(Subcommand)]
enum ProjectCmd {
    /// Create a new project
    Create {
        /// Project name
        name: String,
        /// Project outline text (or file path with @ prefix)
        #[arg(long)]
        outline: Option<String>,
        /// Path to outline file
        #[arg(long)]
        outline_file: Option<PathBuf>,
    },
    /// List all projects
    List,
    /// Show project details (metadata + characters + script summary)
    Show {
        /// Project ID
        project_id: String,
    },
    /// Delete a project and all its data
    Delete {
        /// Project ID
        project_id: String,
    },
    /// Save or update a project's outline
    Outline {
        /// Project ID
        project_id: String,
        /// Outline text (or file path with @ prefix)
        text: Option<String>,
        /// Path to outline file
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

// ============================================================
// Character sub-commands
// ============================================================

#[derive(Subcommand)]
enum CharacterCmd {
    /// List characters for a project
    List {
        /// Project ID
        project_id: String,
    },
    /// Create a new character
    Create {
        /// Project ID
        project_id: String,
        /// Character name
        name: String,
        /// TTS voice name
        #[arg(long)]
        voice: String,
        /// TTS model
        #[arg(long, default_value = "qwen3-tts-instruct-flash-realtime")]
        model: String,
        /// Speech speed (default 1.0)
        #[arg(long, default_value = "1.0")]
        speed: Option<f32>,
        /// Speech pitch (default 1.0)
        #[arg(long, default_value = "1.0")]
        pitch: Option<f32>,
    },
    /// Update an existing character
    Update {
        /// Character ID
        character_id: String,
        /// Character name
        #[arg(long)]
        name: Option<String>,
        /// TTS voice name
        #[arg(long)]
        voice: Option<String>,
        /// TTS model
        #[arg(long)]
        model: Option<String>,
        /// Speech speed
        #[arg(long)]
        speed: Option<f32>,
        /// Speech pitch
        #[arg(long)]
        pitch: Option<f32>,
    },
    /// Delete a character
    Delete {
        /// Character ID
        character_id: String,
    },
    /// Import characters from another project
    Import {
        /// Target project ID
        to_project_id: String,
        /// Source character IDs (comma-separated)
        character_ids: String,
    },
    /// List all characters across all projects
    ListAll,
}

// ============================================================
// Helper types
// ============================================================

#[derive(Serialize)]
struct ProjectInfo {
    id: String,
    name: String,
    outline: String,
    created_at: String,
    updated_at: String,
    character_count: usize,
    line_count: usize,
    section_count: usize,
    audio_count: usize,
}

#[derive(Serialize)]
struct CharacterInfo {
    id: String,
    name: String,
    voice_name: String,
    tts_model: String,
    speed: f32,
    pitch: f32,
}

// ============================================================
// Main
// ============================================================

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let emitter = LogEmitter {
        verbose: cli.verbose,
    };

    let db = Database::open(&cli.db).map_err(|e| format!("Failed to open database: {}", e))?;
    db.migrate()
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

    let db = Mutex::new(db);

    match cli.command {
        // ---- Project commands ----
        Commands::Project { cmd } => match cmd {
            ProjectCmd::Create {
                name,
                outline,
                outline_file,
            } => {
                let id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                let outline_text = resolve_text_or_file(outline.as_deref(), outline_file.as_ref())?;

                let project = Project {
                    id: id.clone(),
                    name: name.clone(),
                    outline: outline_text,
                    global_video_style: String::new(),
                    created_at: now.clone(),
                    updated_at: now,
                };

                let db_lock = lock_db(&db)?;
                db_lock.insert_project(&project)?;

                if cli.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&project)?);
                } else {
                    eprintln!("Created project: {} ({})", name, id);
                    println!("{}", id);
                }
            }

            ProjectCmd::List => {
                let db_lock = lock_db(&db)?;
                let projects = db_lock.list_projects()?;

                if cli.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&projects)?);
                } else if projects.is_empty() {
                    println!("No projects found.");
                } else {
                    for p in &projects {
                        println!("{}\t{}", p.id, p.name);
                    }
                }
            }

            ProjectCmd::Show { project_id } => {
                let db_lock = lock_db(&db)?;
                let project = db_lock.get_project(&project_id)?;
                let characters = db_lock.list_characters(&project_id)?;
                let lines = db_lock.load_script(&project_id)?;
                let sections = db_lock.list_sections(&project_id)?;
                let audio = db_lock.list_audio_fragments(&project_id)?;

                if cli.format == "json" {
                    let info = ProjectInfo {
                        id: project.id,
                        name: project.name,
                        outline: project.outline,
                        created_at: project.created_at,
                        updated_at: project.updated_at,
                        character_count: characters.len(),
                        line_count: lines.len(),
                        section_count: sections.len(),
                        audio_count: audio.len(),
                    };
                    println!("{}", serde_json::to_string_pretty(&info)?);
                } else {
                    println!("Project: {}", project.name);
                    println!("ID: {}", project.id);
                    println!("Created: {}", project.created_at);
                    println!("Updated: {}", project.updated_at);
                    println!();
                    println!("Characters ({}):", characters.len());
                    for c in &characters {
                        println!(
                            "  {}  voice={}  model={}  speed={}  pitch={}",
                            c.name, c.voice_name, c.tts_model, c.speed, c.pitch
                        );
                    }
                    println!();
                    println!("Sections ({}):", sections.len());
                    for s in &sections {
                        println!("  [{}] {}", s.section_order, s.title);
                    }
                    println!();
                    println!("Lines: {}", lines.len());
                    println!("Audio fragments: {}/{}", audio.len(), lines.len());
                }
            }

            ProjectCmd::Delete { project_id } => {
                let db_lock = lock_db(&db)?;
                db_lock.delete_project(&project_id)?;
                eprintln!("Deleted project: {}", project_id);
            }

            ProjectCmd::Outline {
                project_id,
                text,
                file,
            } => {
                let outline_text = resolve_text_or_file(text.as_deref(), file.as_ref())?;
                let db_lock = lock_db(&db)?;
                db_lock.save_project_outline(&project_id, &outline_text)?;
                eprintln!("Outline saved for project: {}", project_id);
            }
        },

        // ---- Character commands ----
        Commands::Character { cmd } => match cmd {
            CharacterCmd::List { project_id } => {
                let db_lock = lock_db(&db)?;
                let characters = db_lock.list_characters(&project_id)?;

                if cli.format == "json" {
                    let info: Vec<CharacterInfo> = characters
                        .into_iter()
                        .map(|c| CharacterInfo {
                            id: c.id,
                            name: c.name,
                            voice_name: c.voice_name,
                            tts_model: c.tts_model,
                            speed: c.speed,
                            pitch: c.pitch,
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&info)?);
                } else if characters.is_empty() {
                    println!("No characters found for project: {}", project_id);
                } else {
                    for c in &characters {
                        println!(
                            "{}\t{}\t{}\t{}\tspeed={}\tpitch={}",
                            c.id, c.name, c.voice_name, c.tts_model, c.speed, c.pitch
                        );
                    }
                }
            }

            CharacterCmd::Create {
                project_id,
                name,
                voice,
                model,
                speed,
                pitch,
            } => {
                let id = uuid::Uuid::new_v4().to_string();
                let character = Character {
                    id: id.clone(),
                    project_id,
                    name,
                    voice_name: voice,
                    tts_model: model,
                    speed: speed.unwrap_or(1.0),
                    pitch: pitch.unwrap_or(1.0),
                };

                let db_lock = lock_db(&db)?;
                db_lock.insert_character(&character)?;

                if cli.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&character)?);
                } else {
                    eprintln!("Created character: {} ({})", character.name, id);
                    println!("{}", id);
                }
            }

            CharacterCmd::Update {
                character_id,
                name,
                voice,
                model,
                speed,
                pitch,
            } => {
                let db_lock = lock_db(&db)?;
                let existing = db_lock.get_character_by_id(&character_id)?;
                let character = Character {
                    id: character_id,
                    project_id: existing.project_id,
                    name: name.unwrap_or(existing.name),
                    voice_name: voice.unwrap_or(existing.voice_name),
                    tts_model: model.unwrap_or(existing.tts_model),
                    speed: speed.unwrap_or(existing.speed),
                    pitch: pitch.unwrap_or(existing.pitch),
                };
                db_lock.update_character(&character)?;

                if cli.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&character)?);
                } else {
                    eprintln!("Updated character: {}", character.name);
                }
            }

            CharacterCmd::Delete { character_id } => {
                let db_lock = lock_db(&db)?;
                db_lock.delete_character(&character_id)?;
                eprintln!("Deleted character: {}", character_id);
            }

            CharacterCmd::Import {
                to_project_id,
                character_ids,
            } => {
                let ids: Vec<String> = character_ids
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
                let db_lock = lock_db(&db)?;
                let mut imported = Vec::new();
                for char_id in &ids {
                    let source = db_lock.get_character_by_id(char_id)?;
                    let new_id = uuid::Uuid::new_v4().to_string();
                    let character = Character {
                        id: new_id,
                        project_id: to_project_id.clone(),
                        name: source.name.clone(),
                        voice_name: source.voice_name.clone(),
                        tts_model: source.tts_model.clone(),
                        speed: source.speed,
                        pitch: source.pitch,
                    };
                    db_lock.insert_character(&character)?;
                    imported.push(character);
                }

                if cli.format == "json" {
                    println!("{}", serde_json::to_string_pretty(&imported)?);
                } else {
                    eprintln!(
                        "Imported {} characters into project: {}",
                        imported.len(),
                        to_project_id
                    );
                }
            }

            CharacterCmd::ListAll => {
                let db_lock = lock_db(&db)?;
                let all = db_lock.list_all_project_characters()?;

                if cli.format == "json" {
                    let flat: Vec<_> = all
                        .iter()
                        .flat_map(|(pid, pname, chars)| {
                            let pid = pid.clone();
                            let pname = pname.clone();
                            chars.iter().map(move |c| {
                                json!({
                                    "project_id": pid,
                                    "project_name": pname,
                                    "id": c.id,
                                    "name": c.name,
                                    "voice_name": c.voice_name,
                                    "tts_model": c.tts_model,
                                    "speed": c.speed,
                                    "pitch": c.pitch,
                                })
                            })
                        })
                        .collect::<Vec<_>>();
                    println!("{}", serde_json::to_string_pretty(&flat)?);
                } else {
                    for (pid, pname, chars) in &all {
                        eprintln!("[{}] {} ({} chars):", pid, pname, chars.len());
                        for c in chars {
                            println!(
                                "  {}\t{}\t{}\tspeed={}\tpitch={}",
                                c.id, c.name, c.voice_name, c.speed, c.pitch
                            );
                        }
                    }
                }
            }
        },

        // ---- Agent Pipeline ----
        Commands::Pipeline {
            ref outline,
            ref project_id,
        } => {
            let (endpoint, key, model) = llm_opts(&cli)?;
            let outline_text = std::fs::read_to_string(outline)
                .map_err(|e| format!("Failed to read outline: {}", e))?;

            let characters = load_characters(&db, project_id)?;

            eprintln!("=== Step 1: Analyzing outline ===");
            let plan = do_outline_analysis(
                &emitter,
                &outline_text,
                &characters,
                endpoint,
                key,
                model,
                cli.enable_thinking,
            )
            .await
            .map_err(|e| format!("Outline analysis failed: {}", e))?;

            eprintln!(
                "\nPlan: {} chapters, {} characters suggested",
                plan.chapters.len(),
                plan.suggested_characters.len()
            );

            if cli.format == "json" {
                println!("{}", serde_json::to_string_pretty(&plan)?);
            }

            eprintln!("\n=== Step 2: Generating script ===");
            let result = do_script_generation(
                &emitter,
                &db,
                project_id,
                &outline_text,
                &characters,
                endpoint,
                key,
                model,
                Some(&plan),
                None,
                cli.enable_thinking,
            )
            .await
            .map_err(|e| format!("Script generation failed: {}", e))?;

            let total_lines: usize = result.sections.iter().map(|s| s.lines.len()).sum();
            eprintln!(
                "\nDone! Generated {} sections with {} lines.",
                result.sections.len(),
                total_lines
            );
        }

        Commands::Generate {
            ref outline,
            ref project_id,
            ref extra,
        } => {
            let (endpoint, key, model) = llm_opts(&cli)?;
            let outline_text = std::fs::read_to_string(outline)
                .map_err(|e| format!("Failed to read outline: {}", e))?;

            let characters = load_characters(&db, project_id)?;

            eprintln!("=== Generating script ===");
            let result = do_script_generation(
                &emitter,
                &db,
                project_id,
                &outline_text,
                &characters,
                endpoint,
                key,
                model,
                None,
                extra.as_deref(),
                cli.enable_thinking,
            )
            .await
            .map_err(|e| format!("Script generation failed: {}", e))?;

            let total_lines: usize = result.sections.iter().map(|s| s.lines.len()).sum();
            eprintln!(
                "\nDone! Generated {} sections with {} lines.",
                result.sections.len(),
                total_lines
            );
        }

        Commands::Analyze {
            ref outline,
            ref project_id,
        } => {
            let (endpoint, key, model) = llm_opts(&cli)?;
            let outline_text = std::fs::read_to_string(outline)
                .map_err(|e| format!("Failed to read outline: {}", e))?;

            let characters = load_characters(&db, project_id)?;

            eprintln!("=== Analyzing outline ===");
            let plan = do_outline_analysis(
                &emitter,
                &outline_text,
                &characters,
                endpoint,
                key,
                model,
                cli.enable_thinking,
            )
            .await
            .map_err(|e| format!("Outline analysis failed: {}", e))?;

            eprintln!(
                "Plan: {} chapters, {} characters suggested",
                plan.chapters.len(),
                plan.suggested_characters.len()
            );

            // Always output plan as JSON for agent consumption
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }

        Commands::Revise {
            ref project_id,
            ref instructions,
            ref sections,
            ref outline,
        } => {
            let (endpoint, key, model) = llm_opts(&cli)?;

            // Load outline from file or project
            let outline_text = if let Some(path) = outline {
                std::fs::read_to_string(path)
                    .map_err(|e| format!("Failed to read outline file: {}", e))?
            } else {
                let db_lock = lock_db(&db)?;
                db_lock.get_project(project_id)?.outline
            };

            let characters = load_characters(&db, project_id)?;

            let section_indices: Option<Vec<usize>> = sections.as_ref().map(|s| {
                s.split(',')
                    .filter_map(|x| x.trim().parse::<usize>().ok())
                    .collect()
            });

            eprintln!("=== Revising script ===");
            if let Some(ref indices) = section_indices {
                eprintln!("Targeting sections: {:?}", indices);
            }

            let extra = format!(
                "REVISION REQUEST: {}\nOnly modify the requested sections.",
                instructions
            );

            let result = do_script_generation(
                &emitter,
                &db,
                project_id,
                &outline_text,
                &characters,
                endpoint,
                key,
                model,
                None,
                Some(&extra),
                cli.enable_thinking,
            )
            .await
            .map_err(|e| format!("Script revision failed: {}", e))?;

            let total_lines: usize = result.sections.iter().map(|s| s.lines.len()).sum();
            eprintln!(
                "\nDone! Revised {} sections with {} lines.",
                result.sections.len(),
                total_lines
            );
        }

        // ---- TTS ----
        Commands::Tts {
            project_id,
            tts_key,
        } => {
            let _api_key = tts_key
                .or_else(|| cli.key.clone())
                .ok_or("Missing --key (or set VOXFLOW_API_KEY or --tts-key)")?;

            eprintln!("=== TTS status for project: {} ===", project_id);

            let db_lock = lock_db(&db)?;
            let lines = db_lock.load_script(&project_id)?;
            let fragments = db_lock.list_audio_fragments(&project_id)?;

            let existing_ids: HashSet<&str> =
                fragments.iter().map(|f| f.line_id.as_str()).collect();

            let missing: Vec<&vox_flow_lib::core::models::ScriptLine> = lines
                .iter()
                .filter(|l| !existing_ids.contains(l.id.as_str()))
                .collect();

            eprintln!("Total lines: {}", lines.len());
            eprintln!("Existing audio: {}", fragments.len());
            eprintln!("Missing audio: {}", missing.len());

            if cli.format == "json" {
                let status = json!({
                    "project_id": project_id,
                    "total_lines": lines.len(),
                    "audio_generated": fragments.len(),
                    "audio_missing": missing.len(),
                    "missing_line_ids": missing.iter().map(|l| &l.id).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else if !missing.is_empty() {
                eprintln!("\nMissing audio for these lines:");
                for line in &missing {
                    let speaker = line.character_id.as_deref().unwrap_or("Narrator");
                    eprintln!(
                        "  [{}] {}: {}",
                        speaker,
                        line.id,
                        line.text.chars().take(60).collect::<String>()
                    );
                }
            }
        }

        // ---- Audio export ----
        Commands::Export {
            project_id,
            output,
            bgm,
            bgm_volume,
        } => {
            let bgm_path = bgm.map(|p| p.to_string_lossy().to_string());
            let vol = bgm_volume.unwrap_or(0.3);
            let output_str = output.to_string_lossy().to_string();

            eprintln!("=== Exporting audio mix ===");
            eprintln!("Output: {}", output_str);
            if let Some(ref bgm) = bgm_path {
                eprintln!("BGM: {} (volume: {})", bgm, vol);
            }

            mix_audio_cli(&db, &project_id, &output_str, bgm_path.as_deref(), vol)?;

            eprintln!("Export complete: {}", output_str);
        }

        Commands::Script { project_id, format } => {
            let fmt = format.unwrap_or_else(|| cli.format.clone());
            let db_lock = lock_db(&db)?;
            let lines = db_lock
                .load_script_lines(&project_id)
                .map_err(|e| format!("Failed to load script: {}", e))?;

            if lines.is_empty() {
                eprintln!("No script found for project '{}'.", project_id);
            } else if fmt == "json" {
                let output: Vec<serde_json::Value> = lines
                    .into_iter()
                    .map(|l| {
                        json!({
                            "order": l.line_order,
                            "text": l.text,
                            "character": l.character_name.unwrap_or_else(|| "Narrator".to_string()),
                            "section": l.section_title,
                            "instructions": l.instructions,
                            "gap_ms": l.gap_after_ms,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                let mut current_section = String::new();
                for line in lines {
                    if let Some(ref section) = line.section_title {
                        if section != &current_section {
                            current_section = section.clone();
                            println!("\n=== {} ===\n", section);
                        }
                    }
                    let speaker = line.character_name.as_deref().unwrap_or("Narrator");
                    println!("[{}] {}", speaker, line.text);
                }
            }
        }

        Commands::AudioStatus { project_id } => {
            let db_lock = lock_db(&db)?;
            let lines = db_lock.load_script(&project_id)?;
            let fragments = db_lock.list_audio_fragments(&project_id)?;

            let existing_ids: HashSet<&str> =
                fragments.iter().map(|f| f.line_id.as_str()).collect();

            let generated = lines
                .iter()
                .filter(|l| existing_ids.contains(l.id.as_str()))
                .count();
            let missing = lines.len() - generated;

            if cli.format == "json" {
                let status = json!({
                    "project_id": project_id,
                    "total_lines": lines.len(),
                    "audio_generated": generated,
                    "audio_missing": missing,
                    "fragments": fragments.iter().map(|f| {
                        json!({
                            "line_id": &f.line_id,
                            "file": &f.file_path,
                            "duration_ms": f.duration_ms,
                        })
                    }).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                eprintln!("Project: {}", project_id);
                eprintln!("Total lines: {}", lines.len());
                eprintln!("Audio generated: {}/{}", generated, lines.len());
                if missing > 0 {
                    eprintln!("Audio missing: {}", missing);
                }
            }
        }
    }

    Ok(())
}

// ============================================================
// Helpers
// ============================================================

fn llm_opts(cli: &Cli) -> Result<(&str, &str, &str)> {
    let endpoint = cli
        .endpoint
        .as_deref()
        .ok_or("Missing --endpoint (or set VOXFLOW_API_ENDPOINT)")?;
    let key = cli
        .key
        .as_deref()
        .ok_or("Missing --key (or set VOXFLOW_API_KEY)")?;
    let model = cli.model.as_deref().unwrap_or("qwen-plus");
    Ok((endpoint, key, model))
}

fn lock_db(db: &Mutex<Database>) -> Result<std::sync::MutexGuard<'_, Database>> {
    Ok(db.lock().map_err(|e| format!("DB lock poisoned: {}", e))?)
}

fn load_characters(db: &Mutex<Database>, project_id: &str) -> Result<Vec<Character>> {
    let db_lock = lock_db(db)?;
    Ok(db_lock
        .load_characters(project_id)
        .map_err(|e| format!("Failed to load characters: {}", e))?)
}

/// Resolve text content from either a direct string (with optional @file prefix) or a file path.
fn resolve_text_or_file(text: Option<&str>, file: Option<&PathBuf>) -> Result<String> {
    if let Some(f) = file {
        return std::fs::read_to_string(f)
            .map_err(|e| format!("Failed to read file {}: {}", f.display(), e).into());
    }
    if let Some(t) = text {
        if let Some(path) = t.strip_prefix('@') {
            return std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read file {}: {}", path, e).into());
        }
        return Ok(t.to_string());
    }
    Err("No text or file provided".into())
}

/// Mix audio using ffmpeg (CLI version without Tauri AppHandle).
fn mix_audio_cli(
    db: &Mutex<Database>,
    project_id: &str,
    output_path: &str,
    bgm_path: Option<&str>,
    bgm_volume: f32,
) -> Result<()> {
    let db_lock = lock_db(db)?;
    let lines = db_lock.load_script(project_id)?;
    let fragments = db_lock.list_audio_fragments(project_id)?;

    if fragments.is_empty() {
        return Err("No audio fragments found for this project".into());
    }

    let frag_map: std::collections::HashMap<&str, &AudioFragment> =
        fragments.iter().map(|f| (f.line_id.as_str(), f)).collect();

    let mut audio_paths: Vec<String> = Vec::new();
    let mut gaps_ms: Vec<i32> = Vec::new();

    for line in &lines {
        if let Some(frag) = frag_map.get(line.id.as_str()) {
            if std::path::Path::new(&frag.file_path).exists() {
                audio_paths.push(frag.file_path.clone());
                gaps_ms.push(line.gap_after_ms);
            }
        }
    }

    if audio_paths.is_empty() {
        return Err("No valid audio fragment files found".into());
    }

    // Build ffmpeg command
    let ffmpeg = detect_ffmpeg()?;
    let mut cmd = std::process::Command::new(&ffmpeg);

    // Add all audio inputs
    for path in &audio_paths {
        cmd.arg("-i").arg(path);
    }

    let input_count = audio_paths.len();

    // Add BGM input if provided
    let has_bgm = bgm_path.is_some();
    if let Some(bgm) = bgm_path {
        cmd.arg("-i").arg(bgm);
    }

    // Build filter complex
    let mut filter_parts: Vec<String> = Vec::new();

    if has_bgm {
        // Concat voice tracks
        let concat_inputs: String = (0..input_count).map(|i| format!("[{}:a]", i)).collect();
        filter_parts.push(format!(
            "{}concat=n={}:v=0:a=1[out_voice]",
            concat_inputs, input_count
        ));
        // BGM volume
        filter_parts.push(format!("[{}:a]volume={}[bgm]", input_count, bgm_volume));
        // Mix
        filter_parts.push(
            "[out_voice][bgm]amix=inputs=2:duration=first:dropout_transition=0[out]".to_string(),
        );

        cmd.arg("-filter_complex").arg(filter_parts.join(";"));
        cmd.arg("-map").arg("[out]");
    } else {
        // Just concat audio files
        let concat_inputs: String = (0..input_count).map(|i| format!("[{}:a]", i)).collect();
        filter_parts.push(format!(
            "{}concat=n={}:v=0:a=1[out]",
            concat_inputs, input_count
        ));

        cmd.arg("-filter_complex").arg(filter_parts.join(";"));
        cmd.arg("-map").arg("[out]");
    }

    cmd.arg("-f").arg("mp3");
    cmd.arg("-y");
    cmd.arg(output_path);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ffmpeg failed: {}",
            stderr.lines().take(5).collect::<Vec<_>>().join("\n")
        )
        .into());
    }

    Ok(())
}

fn detect_ffmpeg() -> Result<String> {
    let candidates = ["ffmpeg", "/usr/bin/ffmpeg", "/usr/local/bin/ffmpeg"];
    for c in &candidates {
        if std::process::Command::new(c)
            .arg("-version")
            .output()
            .is_ok()
        {
            return Ok(c.to_string());
        }
    }
    Err("ffmpeg not found — install it or add to PATH".into())
}
