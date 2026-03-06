use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngramError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Document not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Content too large: {size} bytes (max {max} bytes)")]
    ContentTooLarge { size: usize, max: usize },

    #[error("{0}")]
    Custom(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid virtual path: {0}")]
    InvalidVirtualPath(String),

    #[error("Model load error: {0}")]
    ModelLoad(String),
}

impl From<redb::Error> for EngramError {
    fn from(e: redb::Error) -> Self {
        EngramError::Storage(e.to_string())
    }
}

impl From<redb::DatabaseError> for EngramError {
    fn from(e: redb::DatabaseError) -> Self {
        EngramError::Storage(e.to_string())
    }
}

impl From<redb::TableError> for EngramError {
    fn from(e: redb::TableError) -> Self {
        EngramError::Storage(e.to_string())
    }
}

impl From<redb::TransactionError> for EngramError {
    fn from(e: redb::TransactionError) -> Self {
        EngramError::Storage(e.to_string())
    }
}

impl From<redb::CommitError> for EngramError {
    fn from(e: redb::CommitError) -> Self {
        EngramError::Storage(e.to_string())
    }
}

impl From<redb::StorageError> for EngramError {
    fn from(e: redb::StorageError) -> Self {
        EngramError::Storage(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EngramError>;

impl From<bincode::Error> for EngramError {
    fn from(e: bincode::Error) -> Self {
        EngramError::Serialization(e.to_string())
    }
}
