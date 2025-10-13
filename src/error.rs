use thiserror::Error;

#[derive(Error, Debug)]
pub enum EditorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("JSON parsing error: {0}")]
    JsonParsing(#[from] serde_json::Error),
    #[error("File not found in database: {0}")]
    FileNotFound(String),
    #[error("Markdown scanner error: {0}")]
    Scanner(String),
    #[error("Syntax Highlighting error: {0}")]
    SyntaxHighlighting(String),
    #[error("Invalid backlink: {0}")]
    InvalidBacklink(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Server error: {0}")]
    ServerError(String),
}
