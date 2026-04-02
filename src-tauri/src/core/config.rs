use serde_json::Value;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreBuilder;

use super::error::AppError;

/// Store 文件名，用于保存 API 密钥等配置
const STORE_FILE: &str = "store.json";

/// 配置管理器，封装 tauri-plugin-store 实现 API 密钥的安全本地存储。
///
/// 使用 `api_key_{service}` 格式的键名存储各服务的 API 密钥。
pub struct ConfigManager<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> ConfigManager<R> {
    /// 创建新的 ConfigManager 实例。
    ///
    /// 需要传入 Tauri AppHandle，用于访问 tauri-plugin-store。
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    /// 保存指定服务的 API 密钥。
    ///
    /// 密钥以 `api_key_{service}` 为键名存储到本地 store 文件中。
    /// 如果该服务已有密钥，则覆盖旧值。
    pub fn save_api_key(&self, service: &str, key: &str) -> Result<(), AppError> {
        let store = StoreBuilder::new(&self.app, STORE_FILE)
            .build()
            .map_err(|e| AppError::Config(format!("无法打开配置存储: {e}")))?;

        let store_key = format!("api_key_{service}");
        store
            .set(store_key, Value::String(key.to_string()));

        store
            .save()
            .map_err(|e| AppError::Config(format!("保存配置失败: {e}")))?;

        Ok(())
    }

    /// 加载指定服务的 API 密钥。
    ///
    /// 返回 `Ok(Some(key))` 如果密钥存在，`Ok(None)` 如果未配置。
    pub fn load_api_key(&self, service: &str) -> Result<Option<String>, AppError> {
        let store = StoreBuilder::new(&self.app, STORE_FILE)
            .build()
            .map_err(|e| AppError::Config(format!("无法打开配置存储: {e}")))?;

        let store_key = format!("api_key_{service}");
        let value = store.get(&store_key);

        match value {
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    // Note: tauri-plugin-store requires a running Tauri app context,
    // so unit tests for ConfigManager are not feasible without integration testing.
    // Property tests for API key save/load (Property 4) will be implemented
    // in task 4.2 using a mock or integration test approach.
}
