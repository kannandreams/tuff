use std::{error::Error, fmt};

pub type Result<T> = std::result::Result<T, TuffError>;

#[derive(Debug)]
pub struct TuffError(String);

impl TuffError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for TuffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for TuffError {}

impl From<std::io::Error> for TuffError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for TuffError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<toml::de::Error> for TuffError {
    fn from(error: toml::de::Error) -> Self {
        Self(format!("invalid capability manifest TOML: {error}"))
    }
}

impl From<toml::ser::Error> for TuffError {
    fn from(error: toml::ser::Error) -> Self {
        Self(format!("could not serialize TOML: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuff_error_displays_message() {
        let err = TuffError::new("test error");
        assert_eq!(format!("{}", err), "test error");
    }

    #[test]
    fn tuff_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: TuffError = io_err.into();
        assert!(format!("{}", err).contains("file not found"));
    }

    #[test]
    fn tuff_error_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: TuffError = json_err.into();
        assert!(!format!("{}", err).is_empty());
    }
}
