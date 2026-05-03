//! Error types for the network layer.

use thiserror::Error;

/// Errors produced by `MailBackend` implementations and helpers in this crate.
#[derive(Debug, Error)]
pub enum NetError {
    /// Failed to establish a TCP/TLS connection to the server.
    #[error("connect error: {0}")]
    Connect(String),

    /// Authentication was rejected by the server.
    #[error("auth error: {0}")]
    Auth(String),

    /// Protocol-level error (unexpected response, command failure, etc.).
    #[error("protocol error: {0}")]
    Protocol(String),

    /// TLS handshake or configuration error.
    #[error("tls error: {0}")]
    Tls(String),

    /// Failed to parse a server response or RFC 822 payload.
    #[error("parse error: {0}")]
    Parse(String),

    /// Underlying I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Catch-all for everything else.
    #[error("{0}")]
    Other(String),
}

impl NetError {
    /// Build an `Other` variant from any displayable value.
    pub fn other(msg: impl Into<String>) -> Self {
        NetError::Other(msg.into())
    }
}

/// Convenient `Result` alias used throughout `imt-net`.
pub type Result<T> = std::result::Result<T, NetError>;
