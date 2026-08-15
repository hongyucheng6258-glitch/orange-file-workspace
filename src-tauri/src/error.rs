use serde::Serialize;

/// 应用统一错误类型，通过 IPC 序列化给前端。
#[derive(Debug, Serialize, Clone)]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("db_error", e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::new("io_error", e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("serialize_error", e.to_string())
    }
}

impl From<std::path::StripPrefixError> for AppError {
    fn from(e: std::path::StripPrefixError) -> Self {
        Self::new("path_error", e.to_string())
    }
}

impl From<&str> for AppError {
    fn from(msg: &str) -> Self {
        Self::new("app_error", msg)
    }
}

impl From<String> for AppError {
    fn from(msg: String) -> Self {
        Self::new("app_error", msg)
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self::new("tauri_error", e.to_string())
    }
}

impl From<image::ImageError> for AppError {
    fn from(e: image::ImageError) -> Self {
        Self::new("image_error", e.to_string())
    }
}
