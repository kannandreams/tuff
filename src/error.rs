use std::{error::Error, fmt};

pub type Result<T> = std::result::Result<T, CoralError>;

#[derive(Debug)]
pub struct CoralError(String);

impl CoralError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for CoralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for CoralError {}

impl From<std::io::Error> for CoralError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for CoralError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<toml::de::Error> for CoralError {
    fn from(error: toml::de::Error) -> Self {
        Self(format!("invalid capability manifest TOML: {error}"))
    }
}
