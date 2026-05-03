use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("parse error: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
