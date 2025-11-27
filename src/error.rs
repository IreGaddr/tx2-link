use thiserror::Error;

#[derive(Error, Debug)]
pub enum LinkError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Schema mismatch: expected version {expected}, got {actual}")]
    SchemaMismatch { expected: String, actual: String },

    #[error("Component schema not found: {0}")]
    SchemaNotFound(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("MessagePack encode error: {0}")]
    MsgPackEncode(#[from] rmp_serde::encode::Error),

    #[error("MessagePack decode error: {0}")]
    MsgPackDecode(#[from] rmp_serde::decode::Error),

    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Timeout")]
    Timeout,

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, LinkError>;
