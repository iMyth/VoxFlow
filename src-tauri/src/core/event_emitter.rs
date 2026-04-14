/// Abstract event emitter — implemented by both Tauri AppHandle and CLI logger.
/// This allows the core agent logic to be shared between the Tauri app and CLI binary.
pub trait EventEmitter: Send + Sync {
    /// Emit an event with a name and JSON payload.
    fn emit_json(&self, event: &str, payload: &serde_json::Value);
}

/// Convenience extension trait for ergonomic event emission.
pub trait EmitExt {
    /// Emit any serializable value.
    fn emit<T: serde::Serialize>(&self, event: &str, payload: &T);
}

impl<E: EventEmitter> EmitExt for E {
    fn emit<T: serde::Serialize>(&self, event: &str, payload: &T) {
        if let Ok(val) = serde_json::to_value(payload) {
            self.emit_json(event, &val);
        }
    }
}

/// Logging emitter — writes events to stderr for CLI usage.
#[allow(dead_code)]
pub struct LogEmitter {
    pub verbose: bool,
}

impl EventEmitter for LogEmitter {
    fn emit_json(&self, event: &str, payload: &serde_json::Value) {
        if self.verbose {
            eprintln!("[{}] {}", event, serde_json::to_string_pretty(payload).unwrap_or_default());
        }

        // Always print human-readable summaries for structured events
        match event {
            "agent-step" => {
                if let (Some(step), Some(status)) = (
                    payload.get("step").and_then(|v| v.as_str()),
                    payload.get("status").and_then(|v| v.as_str()),
                ) {
                    match status {
                        "started" => eprintln!("  [started] {}", step),
                        "completed" => eprintln!("  [done] {}", step),
                        _ => eprintln!("  [{}] {}", status, step),
                    }
                }
            }
            "agent-pipeline-started" => {
                if let Some(pid) = payload.get("project_id").and_then(|v| v.as_str()) {
                    eprintln!("Agent pipeline started for project: {}", pid);
                }
            }
            "agent-pipeline-complete" => {
                if let Some(success) = payload.get("success").and_then(|v| v.as_bool()) {
                    if success {
                        eprintln!("Agent pipeline completed successfully.");
                    } else {
                        eprintln!("Agent pipeline failed.");
                    }
                }
            }
            "llm-error" => {
                // All callers pass raw string via &json!(msg)
                if let Some(msg) = payload.as_str() {
                    eprintln!("[error] {}", msg);
                }
            }
            _ => {}
        }
    }
}
