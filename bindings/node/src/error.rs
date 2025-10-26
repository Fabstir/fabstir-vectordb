use napi::bindgen_prelude::*;
use napi_derive::napi;

pub type Result<T> = std::result::Result<T, VectorDBError>;

#[napi]
#[derive(Debug, Clone)]
pub struct VectorDBError {
    pub message: String,
    pub code: String,
}

impl VectorDBError {
    pub fn new(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: code.into(),
        }
    }

    pub fn s5_error(message: impl Into<String>) -> Self {
        Self::new(message, "S5_ERROR")
    }

    pub fn storage_error(message: impl Into<String>) -> Self {
        Self::new(message, "STORAGE_ERROR")
    }

    pub fn index_error(message: impl Into<String>) -> Self {
        Self::new(message, "INDEX_ERROR")
    }

    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self::new(message, "INVALID_CONFIG")
    }

    pub fn session_error(message: impl Into<String>) -> Self {
        Self::new(message, "SESSION_ERROR")
    }
}

impl std::fmt::Display for VectorDBError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for VectorDBError {}

impl From<VectorDBError> for Error {
    fn from(err: VectorDBError) -> Self {
        Error::new(Status::GenericFailure, err.message)
    }
}

impl From<anyhow::Error> for VectorDBError {
    fn from(err: anyhow::Error) -> Self {
        VectorDBError::new(err.to_string(), "INTERNAL_ERROR")
    }
}

// Convert from HybridError
impl From<vector_db::hybrid::HybridError> for VectorDBError {
    fn from(err: vector_db::hybrid::HybridError) -> Self {
        VectorDBError::index_error(err.to_string())
    }
}
