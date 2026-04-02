use serde::Serialize;

/// 统一错误类型，所有后端错误通过此枚举处理。
/// 使用 `thiserror` 派生 `Error` trait，使用 `serde::Serialize` 确保可通过 IPC 传递到前端。
#[derive(Debug, thiserror::Error, Serialize)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("文件系统错误: {0}")]
    FileSystem(String),

    #[error("LLM 服务错误: {0}")]
    LlmService(String),

    #[error("TTS 服务错误: {0}")]
    TtsService(String),

    #[error("FFmpeg 错误: {0}")]
    FFmpeg(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("IPC 序列化错误: {0}")]
    Serialization(String),
}
