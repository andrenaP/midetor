use thiserror::Error;

#[derive(Error, Debug)]
pub enum EditorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File not found in database: {0}")]
    FileNotFound(String),
    #[error("Markdown scanner error: {0}")]
    Scanner(String),
    #[error("Invalid backlink index: {0}")]
    SyntaxHighlighting(String),
    #[error("EditorError: {0}")]
    // InvalidBacklinkIndex(usize),
    InvalidBacklink(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}
