use thiserror::Error;

#[derive(Debug, Error)]
pub enum BseError {
    #[error("config error: {0}")]
    Config(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("provider error: {0}")]
    Provider(#[from] crate::provider::ProviderError),

    #[error("gate error: {0}")]
    Gate(String),

    #[error("audit error: {0}")]
    Audit(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BseError>;
