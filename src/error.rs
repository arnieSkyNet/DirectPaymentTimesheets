use std::fmt;

#[derive(Debug)]
pub enum AppError {
    Config(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Config(message) => write!(f, "Configuration error: {}", message),
        }
    }
}

impl std::error::Error for AppError {}
